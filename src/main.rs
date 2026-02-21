mod config;
mod llm;
mod modes;
mod prompt;

mod tui;

use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "niko",
    version,
    about = "AI-powered CLI: explain code, generate commands, manage LLM providers",
    long_about = "Niko is an AI-powered CLI conversational assistant.\n\n\
    • niko settings configure        — Set up any LLM provider dynamically\n\
    \n\
    Supports: Ollama (local), OpenAI, Claude, DeepSeek, Grok, Groq, Mistral, Together, OpenRouter, and any OpenAI-compatible API."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Override the default LLM provider
    #[arg(short, long, global = true)]
    provider: Option<String>,

    /// Show debug information
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Default mode: remaining args are treated as a command query
    #[arg(trailing_var_arg = true)]
    query: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// View and manage configuration
    Settings {
        #[command(subcommand)]
        action: Option<SettingsAction>,
    },

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum SettingsAction {
    /// Show current configuration
    Show,
    /// Interactive provider setup wizard
    Configure,
    /// Set a specific config value (e.g. `niko settings set openai.model gpt-4o`)
    Set { key: String, value: String },
    /// Re-initialise config to defaults
    Init,
    /// Print the config file path
    Path,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Settings { action }) => {
            let settings_action = match action {
                Some(SettingsAction::Show) => Some(modes::settings::Action::Show),
                Some(SettingsAction::Configure) => Some(modes::settings::Action::Configure),
                Some(SettingsAction::Set { key, value }) => {
                    Some(modes::settings::Action::Set { key, value })
                }
                Some(SettingsAction::Init) => Some(modes::settings::Action::Init),
                Some(SettingsAction::Path) => Some(modes::settings::Action::Path),
                None => None,
            };
            modes::settings::run(settings_action)
        }

        Some(Commands::Version) => {
            println!("niko {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }

        None => {
            if !cli.query.is_empty() {
                run_query_mode(&cli)
            } else {
                // No args — launch TUI Chat
                if let Err(e) = tui::run() {
                    eprintln!("Error launching TUI: {}", e);
                }
                Ok(())
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", "✗".red().bold(), e);
        std::process::exit(1);
    }
}

fn run_query_mode(cli: &Cli) -> anyhow::Result<()> {
    let query = cli.query.join(" ");
    let ctx = prompt::gather_context();
    let mut system = prompt::chat_system_prompt(&ctx);

    let tool_help = prompt::discover_tool_help(&query, cli.verbose);
    if !tool_help.is_empty() {
        system.push_str(&tool_help);
    }

    let messages = vec![
        llm::Message {
            role: llm::Role::System,
            content: system,
        },
        llm::Message {
            role: llm::Role::User,
            content: query,
        },
    ];

    let provider = llm::get_provider(cli.provider.as_deref())?;
    if cli.verbose {
        eprintln!("Using provider: {}", provider.name());
    }

    let response = llm::generate_with_retry(provider.as_ref(), &messages, 2048)?;
    println!("{response}");
    Ok(())
}
