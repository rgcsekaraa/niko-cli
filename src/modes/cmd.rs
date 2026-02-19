use anyhow::{Result, bail};
use colored::*;

use crate::{llm, prompt, safety, ui};

/// Run the /cmd mode — translate natural language to shell commands
pub fn run(query: &str, provider_override: Option<&str>, verbose: bool) -> Result<()> {
    if query.trim().is_empty() {
        bail!("Please provide a query.\nUsage: niko cmd \"find all large files\"");
    }

    // Get provider
    let provider = llm::get_provider(provider_override)?;

    if !provider.is_available() {
        ui::print_warning(&format!("Provider '{}' not ready", provider.name()));
        eprintln!("  Run: {}", "niko settings configure".cyan());
        return Ok(());
    }

    if verbose {
        eprintln!(
            "{} provider: {}",
            "debug".dimmed(),
            provider.name().dimmed()
        );
    }

    // Gather system context and build prompts
    let ctx = prompt::gather_context();
    let system_prompt = prompt::cmd_system_prompt(&ctx);

    // Show spinner while generating
    let mut spinner = ui::Spinner::new("Thinking...");
    spinner.start();

    let start = std::time::Instant::now();
    let response = provider.generate(&system_prompt, query);
    spinner.stop();

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            ui::print_error("Generation failed");
            ui::print_dim(&format!("  {}", e));
            return Ok(());
        }
    };

    if verbose {
        ui::print_dim(&format!("  response time: {:?}", start.elapsed()));
    }

    let command = safety::extract_command(&response);
    if command.is_empty() {
        ui::print_warning("Could not generate a command");
        ui::print_dim("  Try being more specific");
        return Ok(());
    }

    // Handle declined/special messages
    if command.starts_with("Declined:")
        || command.starts_with("Please specify:")
        || command.starts_with("echo \"Declined:")
        || command.starts_with("echo \"Please specify:")
    {
        let msg = command
            .strip_prefix("echo \"")
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(&command);
        ui::print_warning(msg);
        return Ok(());
    }

    // Check if the tool exists
    if let Some(tool) = safety::get_first_tool(&command) {
        if !safety::is_tool_available(&tool) {
            ui::print_dim(&format!("  '{}' not found — install it first", tool));
        }
    }

    // Display command in a bordered box
    ui::display_command(&command);

    // Copy to clipboard
    if ui::copy_to_clipboard(&command) {
        ui::print_dim("  Copied to clipboard ✓");
    }

    // Safety warning
    let risk = safety::assess_risk(&command);
    match risk {
        safety::RiskLevel::Critical => {
            eprintln!();
            ui::print_warning("⚠ Destructive command — review carefully before running");
        }
        safety::RiskLevel::Dangerous => {
            eprintln!();
            ui::print_dim("  ⚠ Review before running");
        }
        _ => {}
    }

    eprintln!();
    Ok(())
}
