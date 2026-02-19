use std::fs;

use anyhow::{Result, bail};
use colored::*;

use crate::{chunker, llm, ui};

/// Run the /explain mode — explain code with chunking for large inputs
pub fn run(file_path: Option<&str>, provider_override: Option<&str>, verbose: bool) -> Result<()> {
    // Read the code input
    let code = if let Some(path) = file_path {
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?;

        let line_count = content.lines().count();
        eprintln!();
        ui::box_top(&format!("{}", format!("File: {}", path).dimmed()));
        ui::box_line(&format!(
            "{}",
            format!("{} lines loaded", line_count).cyan()
        ));
        ui::box_bottom();

        content
    } else {
        ui::read_stdin_input()
            .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?
    };

    let code = code.trim().to_string();

    if code.is_empty() {
        bail!(
            "No code provided.\n\n\
             Usage:\n\
             \x20 niko explain -f <file>           # Explain a file\n\
             \x20 cat file.rs | niko explain       # Pipe code in\n\
             \x20 niko explain                     # Paste interactively"
        );
    }

    // Show collapsible code preview
    ui::show_code_preview(&code);

    let line_count = code.lines().count();
    eprintln!();
    eprintln!(
        "  {} Analyzing {} lines...",
        "📖".to_string(),
        line_count.to_string().cyan()
    );

    // Get provider
    let provider = llm::get_provider(provider_override)?;

    if !provider.is_available() {
        ui::print_warning(&format!("Provider '{}' not ready", provider.name()));
        eprintln!("  Run: {}", "niko settings configure".cyan());
        return Ok(());
    }

    if verbose {
        ui::print_dim(&format!("  provider: {}", provider.name()));
    }

    // Process with chunking engine
    let mut spinner = ui::Spinner::new("Analyzing code...");
    spinner.start();

    let result = chunker::explain_code(&code, provider.as_ref(), verbose);
    spinner.stop();

    match result {
        Ok(explanation) => {
            ui::display_explanation(&explanation);
        }
        Err(e) => {
            ui::print_error("Code analysis failed");
            ui::print_dim(&format!("  {}", e));
        }
    }

    Ok(())
}
