use std::io::{self, Write};

use anyhow::Result;
use colored::*;

use crate::config::{self, ProviderConfig};
use crate::llm;
use crate::llm::Provider;
use crate::llm::ollama;
use crate::ui;

/// Settings action types
pub enum Action {
    Show,
    Configure,
    Set { key: String, value: String },
    Init,
    Path,
}

/// Run the /settings mode
pub fn run(action: Option<Action>) -> Result<()> {
    match action {
        Some(Action::Show) | None => show_config(),
        Some(Action::Configure) => run_configure_wizard(),
        Some(Action::Set { key, value }) => set_config(&key, &value),
        Some(Action::Init) => init_config(),
        Some(Action::Path) => {
            println!("{}", config::config_path().display());
            Ok(())
        }
    }
}

// ─── Show ───────────────────────────────────────────────────────────────────

fn show_config() -> Result<()> {
    let cfg = config::load()?;

    eprintln!();

    // Header
    ui::box_top(&format!("{}", "Niko Configuration".bold()));
    ui::box_empty();
    ui::box_kv("  Config", &format!("{}", config::config_path().display()));
    ui::box_kv(
        "  System",
        &format!("{}GB RAM  •  {} cores", config::system_ram_gb(), config::cpu_count()),
    );
    ui::box_kv(
        "  Limit ",
        &format!("~{}B parameters max", config::max_model_size_for_ram()),
    );

    ui::box_sep();

    // Active provider
    ui::box_kv_bold(
        "  Active",
        &cfg.active_provider.cyan().bold().to_string(),
    );

    ui::box_sep();

    // Providers
    if cfg.providers.is_empty() {
        ui::box_line(&"  (no providers configured)".dimmed().to_string());
    }

    let provider_names: Vec<_> = cfg.providers.keys().cloned().collect();
    for (i, name) in provider_names.iter().enumerate() {
        let pcfg = &cfg.providers[name];
        if i > 0 {
            ui::box_sep();
        }

        let active_badge = if name == &cfg.active_provider {
            " ✓".green().to_string()
        } else {
            String::new()
        };
        let kind_badge = format!("({})", pcfg.kind).dimmed().to_string();

        ui::box_line(&format!(
            "  {} {} {}{}",
            "▸".dimmed(),
            name.bold(),
            kind_badge,
            active_badge
        ));

        if pcfg.kind == "ollama" {
            let status = if ollama::is_ollama_running() {
                "● running".green().to_string()
            } else if ollama::is_ollama_installed() {
                "○ stopped".yellow().to_string()
            } else {
                "✗ not installed".red().to_string()
            };
            ui::box_kv("    Status", &status);
            ui::box_kv("    URL   ", &pcfg.base_url.dimmed().to_string());
        } else {
            ui::box_kv("    Key   ", &format_key(&pcfg.api_key));
            ui::box_kv("    URL   ", &pcfg.base_url.dimmed().to_string());
        }

        if pcfg.model.is_empty() {
            ui::box_kv("    Model ", &"(not selected)".yellow().to_string());
        } else {
            ui::box_kv("    Model ", &pcfg.model.cyan().to_string());
        }
    }

    ui::box_sep();
    ui::box_line(&"  niko settings configure  — setup providers".dimmed().to_string());
    ui::box_line(&"  niko settings set <key>  — change values".dimmed().to_string());
    ui::box_bottom();
    eprintln!();

    Ok(())
}

// ─── Interactive configure wizard ───────────────────────────────────────────

fn run_configure_wizard() -> Result<()> {
    let templates = config::known_provider_templates();

    eprintln!();
    ui::box_top(&format!("{}", "Configure Provider".bold()));
    ui::box_empty();
    ui::box_line(&"Select a provider to configure:".to_string());
    ui::box_empty();

    for (i, (name, _, _, _)) in templates.iter().enumerate() {
        let tag = if *name == "ollama" {
            "local, free".green().to_string()
        } else {
            "API key".dimmed().to_string()
        };
        ui::box_line(&format!("  {}  {}  {}", format!("{:>2}.", i + 1).dimmed(), name.bold(), format!("({})", tag)));
    }
    ui::box_line(&format!(
        "  {}  {}",
        format!("{:>2}.", templates.len() + 1).dimmed(),
        "Custom OpenAI-compatible endpoint".dimmed()
    ));
    ui::box_empty();
    ui::box_bottom();
    eprintln!();

    let choice = prompt_input("  Choose [1]: ")?;
    let choice: usize = choice.trim().parse().unwrap_or(1);

    if choice == 0 || choice > templates.len() + 1 {
        ui::print_warning("Invalid selection");
        return Ok(());
    }

    if choice == templates.len() + 1 {
        return configure_custom();
    }

    let (name, kind, base_url, env_var) = templates[choice - 1];

    if kind == "ollama" {
        return configure_ollama(name, base_url);
    }

    configure_api_provider(name, kind, base_url, env_var)
}

fn configure_ollama(name: &str, default_url: &str) -> Result<()> {
    eprintln!();
    ui::box_top(&format!("{}", "Ollama (Local)".bold()));

    // Check if ollama is installed
    if !ollama::is_ollama_installed() {
        ui::box_empty();
        ui::box_line(&format!("  Ollama is {}", "not installed".red()));
        ui::box_bottom();
        eprintln!();

        let install = prompt_input("  Install Ollama now? [Y/n]: ")?;
        if install.trim().is_empty() || install.trim().to_lowercase().starts_with('y') {
            ollama::install_ollama()?;
        } else {
            ui::print_dim("  Install from: https://ollama.com/download");
            return Ok(());
        }
    }

    // Check if server is running
    if !ollama::is_ollama_running() {
        ui::box_empty();
        ui::box_line(&format!(
            "  Ollama installed but {}",
            "not running".yellow()
        ));
        ui::box_line(&format!("  Start with: {}", "ollama serve".cyan()));
        ui::box_bottom();
        eprintln!();

        let wait = prompt_input("  Start Ollama and press Enter, or 'q' to skip: ")?;
        if wait.trim() == "q" {
            config::upsert_provider(name, ProviderConfig {
                kind: "ollama".into(),
                base_url: default_url.into(),
                ..Default::default()
            })?;
            config::set_active_provider(name)?;
            ui::print_success("Saved (select model later)");
            return Ok(());
        }

        if !ollama::is_ollama_running() {
            config::upsert_provider(name, ProviderConfig {
                kind: "ollama".into(),
                base_url: default_url.into(),
                ..Default::default()
            })?;
            config::set_active_provider(name)?;
            ui::print_warning("Still not running. Config saved anyway.");
            return Ok(());
        }
    }

    // System info
    let max_b = config::max_model_size_for_ram();
    ui::box_empty();
    ui::box_kv(
        "  RAM      ",
        &format!("{}GB", config::system_ram_gb()),
    );
    ui::box_kv(
        "  Max model",
        &format!("~{}B parameters", max_b),
    );
    ui::box_sep();

    // List local models
    let provider = llm::ollama::OllamaProvider::new(default_url, "")?;
    let local_models = provider.list_models().unwrap_or_default();

    if !local_models.is_empty() {
        ui::box_line(&"  Local models:".dimmed().to_string());
        ui::box_empty();
        for (i, m) in local_models.iter().enumerate() {
            let warn = if m.param_billions > 0.0 && !llm::model_fits_in_ram(m.param_billions) {
                " ⚠".yellow().to_string()
            } else {
                String::new()
            };
            ui::box_line(&format!("  {}  {}{}", format!("{:>2}.", i + 1).dimmed(), m, warn));
        }
        ui::box_sep();
    }

    // Downloadable models
    ui::box_line(&"  Available for download:".dimmed().to_string());
    ui::box_empty();

    let downloadable = ollama::search_ollama_models("")?;
    let mut filtered: Vec<_> = downloadable
        .iter()
        .filter(|m| !local_models.iter().any(|l| l.id == m.id))
        .collect();
    filtered.truncate(15);

    for (i, m) in filtered.iter().enumerate() {
        let num = local_models.len() + i + 1;
        ui::box_line(&format!("  {}  {} {}", format!("{:>2}.", num).dimmed(), m, "↓".dimmed()));
    }

    ui::box_empty();
    ui::box_bottom();
    eprintln!();

    let model_choice = prompt_input("  Select model number or name: ")?;
    let model_choice = model_choice.trim();

    let selected_model = if let Ok(num) = model_choice.parse::<usize>() {
        if num >= 1 && num <= local_models.len() {
            local_models[num - 1].id.clone()
        } else if num > local_models.len() && num <= local_models.len() + filtered.len() {
            filtered[num - local_models.len() - 1].id.clone()
        } else {
            model_choice.to_string()
        }
    } else {
        model_choice.to_string()
    };

    if selected_model.is_empty() {
        ui::print_warning("No model selected");
        return Ok(());
    }

    // RAM warning
    let param_b = llm::estimate_param_billions(&selected_model, 0);
    if param_b > 0.0 && !llm::model_fits_in_ram(param_b) {
        eprintln!();
        ui::print_warning(&format!(
            "Model '{}' ({:.0}B params) may exceed {}GB RAM",
            selected_model, param_b, config::system_ram_gb()
        ));
        let proceed = prompt_input("  Continue anyway? [y/N]: ")?;
        if !proceed.trim().to_lowercase().starts_with('y') {
            ui::print_dim("  Cancelled");
            return Ok(());
        }
    }

    config::upsert_provider(name, ProviderConfig {
        kind: "ollama".into(),
        base_url: default_url.into(),
        model: selected_model.clone(),
        ..Default::default()
    })?;
    config::set_active_provider(name)?;

    eprintln!();
    ui::print_success(&format!(
        "Configured {} → {}",
        name.bold(),
        selected_model.cyan()
    ));
    eprintln!();

    Ok(())
}

fn configure_api_provider(name: &str, kind: &str, default_url: &str, env_var: &str) -> Result<()> {
    eprintln!();
    ui::box_top(&format!("{}", format!("Configure {}", name).bold()));

    // Check for env var
    let existing_key = if !env_var.is_empty() {
        std::env::var(env_var).ok()
    } else {
        None
    };

    let api_key = if let Some(ref key) = existing_key {
        ui::box_empty();
        ui::box_line(&format!("  Found key in {}", format!("${}", env_var).cyan()));
        ui::box_bottom();
        eprintln!();

        let use_it = prompt_input("  Use this key? [Y/n]: ")?;
        if use_it.trim().is_empty() || use_it.trim().to_lowercase().starts_with('y') {
            key.clone()
        } else {
            prompt_input(&format!("  {} API key: ", name))?
                .trim()
                .to_string()
        }
    } else {
        ui::box_empty();
        ui::box_line(&format!("  {} required", "API key".bold()));
        if !env_var.is_empty() {
            ui::box_line(&format!("  Or set: {}", format!("export {}=...", env_var).dimmed()));
        }
        ui::box_bottom();
        eprintln!();

        prompt_input(&format!("  {} API key: ", name))?
            .trim()
            .to_string()
    };

    if api_key.is_empty() {
        ui::print_warning("No API key provided");
        return Ok(());
    }

    // Save immediately so we can fetch models
    let pcfg = ProviderConfig {
        kind: kind.into(),
        api_key: api_key.clone(),
        base_url: default_url.into(),
        model: String::new(),
        ..Default::default()
    };
    config::upsert_provider(name, pcfg.clone())?;

    // Fetch models
    eprintln!();
    let mut spinner = ui::Spinner::new("Fetching models...");
    spinner.start();

    let provider = llm::from_config(name, &pcfg)?;
    let models_result = provider.list_models();
    spinner.stop();

    match models_result {
        Ok(models) if !models.is_empty() => {
            ui::box_top(&format!("{}", format!("{} Models", name).dimmed()));
            ui::box_empty();

            let display: Vec<_> = models.iter().take(30).collect();
            for (i, m) in display.iter().enumerate() {
                let warn = if m.param_billions > 0.0 && !llm::model_fits_in_ram(m.param_billions) {
                    " ⚠".yellow().to_string()
                } else {
                    String::new()
                };
                ui::box_line(&format!("  {}  {}{}", format!("{:>2}.", i + 1).dimmed(), m.name, warn));
            }
            if models.len() > 30 {
                ui::box_line(&format!("  ... and {} more", models.len() - 30));
            }

            ui::box_empty();
            ui::box_bottom();
            eprintln!();

            let choice = prompt_input("  Select model number or ID: ")?;
            let choice = choice.trim();

            let selected = if let Ok(num) = choice.parse::<usize>() {
                if num >= 1 && num <= display.len() {
                    display[num - 1].id.clone()
                } else {
                    choice.to_string()
                }
            } else {
                choice.to_string()
            };

            if !selected.is_empty() {
                config::set_provider_field(name, "model", &selected)?;
                config::set_active_provider(name)?;
                eprintln!();
                ui::print_success(&format!(
                    "Configured {} → {}",
                    name.bold(),
                    selected.cyan()
                ));
            }
        }
        Ok(_) => {
            ui::print_dim("  No models returned");
            let model = prompt_input("  Model ID: ")?.trim().to_string();
            if !model.is_empty() {
                config::set_provider_field(name, "model", &model)?;
                config::set_active_provider(name)?;
                ui::print_success(&format!(
                    "Configured {} → {}",
                    name.bold(),
                    model.cyan()
                ));
            }
        }
        Err(e) => {
            ui::print_warning(&format!("Could not fetch models: {}", e));
            let model = prompt_input("  Model ID: ")?.trim().to_string();
            if !model.is_empty() {
                config::set_provider_field(name, "model", &model)?;
                config::set_active_provider(name)?;
                ui::print_success(&format!(
                    "Configured {} → {}",
                    name.bold(),
                    model.cyan()
                ));
            }
        }
    }

    eprintln!();
    Ok(())
}

fn configure_custom() -> Result<()> {
    eprintln!();
    ui::box_top(&format!("{}", "Custom Endpoint".bold()));
    ui::box_empty();
    ui::box_line(&"OpenAI-compatible API endpoint".dimmed().to_string());
    ui::box_bottom();
    eprintln!();

    let name = prompt_input("  Provider name: ")?
        .trim()
        .to_lowercase();
    if name.is_empty() {
        return Ok(());
    }

    let base_url = prompt_input("  Base URL: ")?
        .trim()
        .to_string();
    if base_url.is_empty() {
        return Ok(());
    }

    let api_key = prompt_input("  API key (blank if none): ")?
        .trim()
        .to_string();

    let model = prompt_input("  Model: ")?
        .trim()
        .to_string();

    config::upsert_provider(&name, ProviderConfig {
        kind: "openai_compat".into(),
        api_key,
        base_url,
        model: model.clone(),
        ..Default::default()
    })?;
    config::set_active_provider(&name)?;

    eprintln!();
    ui::print_success(&format!(
        "Configured {} → {}",
        name.bold(),
        model.cyan()
    ));
    eprintln!();

    Ok(())
}

// ─── Set ────────────────────────────────────────────────────────────────────

fn set_config(key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.splitn(2, '.').collect();

    if parts.len() == 1 {
        match key {
            "active_provider" | "provider" => {
                config::set_active_provider(value)?;
                ui::print_success(&format!("Active provider → {}", value.cyan()));
            }
            _ => {
                anyhow::bail!(
                    "Unknown setting: {}\nUsage: niko settings set <provider>.<field> <value>",
                    key
                );
            }
        }
    } else {
        let provider = parts[0];
        let field = parts[1];
        config::set_provider_field(provider, field, value)?;

        if field.contains("key") {
            ui::print_success(&format!("{}.{} → {}", provider, field, "configured".green()));
        } else {
            ui::print_success(&format!("{}.{} → {}", provider, field, value.cyan()));
        }
    }

    Ok(())
}

// ─── Init ───────────────────────────────────────────────────────────────────

fn init_config() -> Result<()> {
    let cfg = config::default_config();
    config::save(&cfg)?;

    ui::print_success(&format!("Config created: {}", config::config_path().display()));
    ui::print_dim("  Run 'niko settings configure' to set up a provider");

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn format_key(key: &str) -> String {
    if key.is_empty() {
        "–".dimmed().to_string()
    } else if key.len() > 8 {
        format!("{}…{}", &key[..4], &key[key.len() - 4..])
    } else {
        "••••".into()
    }
}

fn prompt_input(prompt: &str) -> Result<String> {
    eprint!("{}", prompt);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim_end_matches('\n').trim_end_matches('\r').to_string())
}
