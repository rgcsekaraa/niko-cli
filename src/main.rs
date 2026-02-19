mod chunker;
mod config;
mod llm;
mod modes;
mod prompt;
mod safety;
mod ui;

use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "niko",
    version,
    about = "AI-powered CLI: explain code, generate commands, manage LLM providers",
    long_about = "Niko is an AI-powered CLI tool with three modes:\n\n\
    • niko cmd \"find large files\"   — Generate shell commands from natural language\n\
    • niko explain                   — Explain code (paste or pipe, handles any size)\n\
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
    /// Generate a shell command from natural language (default mode)
    Cmd {
        /// Natural language description of the command you need
        #[arg(trailing_var_arg = true)]
        query: Vec<String>,

        /// Override the default LLM provider
        #[arg(short, long)]
        provider: Option<String>,

        /// Show debug information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Explain code — paste or pipe code of any size
    Explain {
        /// Optional file path to explain (otherwise reads from stdin)
        #[arg(short, long)]
        file: Option<String>,

        /// Override the default LLM provider
        #[arg(short, long)]
        provider: Option<String>,

        /// Show debug information
        #[arg(short, long)]
        verbose: bool,
    },

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
        Some(Commands::Cmd {
            query,
            provider,
            verbose,
        }) => {
            let provider_ref = provider.as_deref().or(cli.provider.as_deref());
            let query_str = query.join(" ");
            modes::cmd::run(&query_str, provider_ref, verbose || cli.verbose)
        }

        Some(Commands::Explain {
            file,
            provider,
            verbose,
        }) => {
            let provider_ref = provider.as_deref().or(cli.provider.as_deref());
            modes::explain::run(file.as_deref(), provider_ref, verbose || cli.verbose)
        }

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
            // Default mode: if args provided, treat as cmd
            if !cli.query.is_empty() {
                let query_str = cli.query.join(" ");
                modes::cmd::run(&query_str, cli.provider.as_deref(), cli.verbose)
            } else {
                // No args — show help
                use clap::CommandFactory;
                Cli::command().print_help().ok();
                println!();
                Ok(())
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", "✗".red().bold(), e);
        std::process::exit(1);
    }
}
