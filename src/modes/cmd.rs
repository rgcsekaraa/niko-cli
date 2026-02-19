use anyhow::{bail, Result};
use colored::*;

use crate::{llm, prompt, safety, ui};

/// Max tokens for command generation — commands are short, 512 is plenty
const CMD_MAX_TOKENS: u32 = 512;

/// Generate a command string from a natural language query (Pure logic, no UI)
pub fn generate_command(
    query: &str,
    provider_override: Option<&str>,
    verbose: bool,
) -> Result<String> {
    if query.trim().is_empty() {
        bail!("Please provide a query.");
    }

    let provider = llm::get_provider(provider_override)?;

    if !provider.is_available() {
        bail!(
            "Provider '{}' not ready. Run 'niko settings configure'.",
            provider.name()
        );
    }

    let ctx = prompt::gather_context();
    let mut system_prompt = prompt::cmd_system_prompt(&ctx);

    // Dynamic help discovery: run --help for tools mentioned in query
    let help_context = prompt::discover_tool_help(query, verbose);
    if !help_context.is_empty() {
        system_prompt.push_str(&help_context);
    }

    // Non-streaming with retry — we need the full command to extract it safely
    let response =
        llm::generate_with_retry(provider.as_ref(), &system_prompt, query, CMD_MAX_TOKENS)?;

    let command = safety::extract_command(&response);
    if command.is_empty() {
        bail!("Could not generate a command. Try being more specific.");
    }

    Ok(command)
}

/// Run the /cmd mode — translate natural language to shell commands (CLI wrapper)
pub fn run(query: &str, provider_override: Option<&str>, verbose: bool) -> Result<()> {
    if query.trim().is_empty() {
        bail!("Please provide a query.\nUsage: niko cmd \"find all large files\"");
    }

    // Check provider availability early to give specific UI feedback
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

    let mut item = ui::Spinner::new("Thinking...");
    item.start();

    let start = std::time::Instant::now();
    let result = generate_command(query, provider_override, verbose);
    item.stop();

    let elapsed = start.elapsed();

    match result {
        Ok(command) => {
            if verbose {
                ui::print_dim(&format!("  response time: {:?}", elapsed));
            }

            // Check for declined/unsafe responses
            if command.starts_with("Declined:")
                || command.starts_with("Please specify:")
                || command.starts_with("echo \"Declined:")
            {
                let msg = command
                    .strip_prefix("echo \"")
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(&command);
                ui::print_warning(msg);
                return Ok(());
            }

            if let Some(tool) = safety::get_first_tool(&command) {
                if !safety::is_tool_available(&tool) {
                    ui::print_dim(&format!("  '{}' not found — install it first", tool));
                }
            }

            ui::display_command(&command);

            if ui::copy_to_clipboard(&command) {
                ui::print_dim("  Copied to clipboard ✓");
            }

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
        }
        Err(e) => {
            ui::print_error("Generation failed");
            ui::print_dim(&format!("  {}", e));
        }
    }

    Ok(())
}
