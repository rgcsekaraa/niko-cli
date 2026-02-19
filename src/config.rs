use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sysinfo::System;

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Top-level config — fully dynamic, no hardcoded providers or models
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Currently active provider name (e.g. "ollama", "openai", "claude", etc.)
    pub active_provider: String,

    /// Map of provider name → provider config (fully dynamic)
    pub providers: HashMap<String, ProviderConfig>,

    /// Safety settings
    pub safety: SafetyConfig,

    /// UI preferences
    pub ui: UiConfig,
}

/// A single provider configuration — fully dynamic
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderConfig {
    /// Provider kind: "ollama", "openai_compat", "anthropic"
    pub kind: String,

    /// API key (empty for local providers)
    pub api_key: String,

    /// Base URL for the API
    pub base_url: String,

    /// Currently selected model name
    pub model: String,

    /// Additional provider-specific options
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SafetyConfig {
    pub require_confirm_dangerous: bool,
    pub blocked_commands: Vec<String>,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            require_confirm_dangerous: true,
            blocked_commands: vec![
                "rm -rf /".into(),
                "rm -rf /*".into(),
                ":(){ :|:& };:".into(),
                "dd if=/dev/zero of=/dev/sda".into(),
                "mkfs.ext4 /dev/sda".into(),
                "> /dev/sda".into(),
                "chmod -R 777 /".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub color: bool,
    pub verbose: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color: true,
            verbose: false,
        }
    }
}

// ─── Well-known provider templates ──────────────────────────────────────────

/// Returns a list of well-known provider templates for the setup wizard
pub fn known_provider_templates() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    // (name, kind, default_base_url, env_var_for_key)
    vec![
        ("ollama",     "ollama",        "http://127.0.0.1:11434", ""),
        ("openai",     "openai_compat", "https://api.openai.com/v1", "OPENAI_API_KEY"),
        ("claude",     "anthropic",     "https://api.anthropic.com", "ANTHROPIC_API_KEY"),
        ("deepseek",   "openai_compat", "https://api.deepseek.com/v1", "DEEPSEEK_API_KEY"),
        ("grok",       "openai_compat", "https://api.x.ai/v1", "GROK_API_KEY"),
        ("groq",       "openai_compat", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
        ("together",   "openai_compat", "https://api.together.xyz/v1", "TOGETHER_API_KEY"),
        ("mistral",    "openai_compat", "https://api.mistral.ai/v1", "MISTRAL_API_KEY"),
        ("openrouter", "openai_compat", "https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
    ]
}

// ─── Paths ──────────────────────────────────────────────────────────────────

pub fn config_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".niko")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.yaml")
}

// ─── System info ────────────────────────────────────────────────────────────

pub fn system_ram_gb() -> u64 {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.total_memory() / (1024 * 1024 * 1024)
}

pub fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Estimate the max model size (in billions of parameters) this system can handle
pub fn max_model_size_for_ram() -> u64 {
    let ram = system_ram_gb();
    // Rule of thumb: ~1GB VRAM/RAM per 1B parameters (Q4 quantised)
    // Leave ~4GB for OS/apps
    if ram >= 4 {
        ram - 4
    } else {
        1
    }
}

// ─── Default config ─────────────────────────────────────────────────────────

pub fn default_config() -> Config {
    let mut providers = HashMap::new();

    // Only add Ollama as a default offline provider — no hardcoded models
    providers.insert("ollama".into(), ProviderConfig {
        kind: "ollama".into(),
        api_key: String::new(),
        base_url: "http://127.0.0.1:11434".into(),
        model: String::new(), // will be selected dynamically
        options: HashMap::new(),
    });

    Config {
        active_provider: "ollama".into(),
        providers,
        safety: SafetyConfig::default(),
        ui: UiConfig::default(),
    }
}

// ─── Load / Save ────────────────────────────────────────────────────────────

pub fn load() -> Result<Config> {
    let path = config_path();
    let dir = config_dir();

    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config directory: {}", dir.display()))?;

    if !path.exists() {
        let cfg = default_config();
        save(&cfg)?;
        return Ok(cfg);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;

    let mut cfg: Config = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse config YAML")?;

    // Overlay env vars on matching providers
    for (name, _, _, env_var) in known_provider_templates() {
        if !env_var.is_empty() {
            if let Ok(key) = std::env::var(env_var) {
                if let Some(p) = cfg.providers.get_mut(name) {
                    if p.api_key.is_empty() {
                        p.api_key = key;
                    }
                }
            }
        }
    }

    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    let dir = config_dir();

    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config directory: {}", dir.display()))?;

    let yaml = serde_yaml::to_string(cfg)
        .with_context(|| "Failed to serialize config")?;

    fs::write(&path, yaml)
        .with_context(|| format!("Failed to write config: {}", path.display()))?;

    Ok(())
}

/// Cached global config
pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| load().unwrap_or_else(|_| default_config()))
}

// ─── Mutators ───────────────────────────────────────────────────────────────

/// Set the active provider
pub fn set_active_provider(name: &str) -> Result<()> {
    let mut cfg = load()?;
    if !cfg.providers.contains_key(name) {
        anyhow::bail!(
            "Provider '{}' not configured.\nRun 'niko settings configure' to add it.",
            name
        );
    }
    cfg.active_provider = name.into();
    save(&cfg)
}

/// Add or update a provider
pub fn upsert_provider(name: &str, pcfg: ProviderConfig) -> Result<()> {
    let mut cfg = load()?;
    cfg.providers.insert(name.to_string(), pcfg);
    save(&cfg)
}

/// Set a specific field on a provider
pub fn set_provider_field(provider: &str, field: &str, value: &str) -> Result<()> {
    let mut cfg = load()?;
    let p = cfg.providers.entry(provider.to_string()).or_default();

    match field {
        "api_key" => p.api_key = value.into(),
        "base_url" => p.base_url = value.into(),
        "model" => p.model = value.into(),
        "kind" => p.kind = value.into(),
        _ => {
            p.options.insert(field.to_string(), value.to_string());
        }
    }

    save(&cfg)
}

/// Get the active provider config
pub fn active_provider() -> Result<(String, ProviderConfig)> {
    let cfg = load()?;
    let name = &cfg.active_provider;
    let pcfg = cfg.providers.get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!(
            "Active provider '{}' not found in config.\nRun 'niko settings configure' to set up a provider.",
            name
        ))?;
    Ok((name.clone(), pcfg))
}
