pub mod ollama;
pub mod openai_compat;
pub mod claude;

use anyhow::{Result, bail};

use crate::config::{self, ProviderConfig};

/// Trait for all LLM providers
pub trait Provider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Generate a response from the LLM
    fn generate(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;

    /// Check if the provider is available
    fn is_available(&self) -> bool;

    /// Fetch all available models from this provider (API call)
    fn list_models(&self) -> Result<Vec<ModelInfo>>;
}

/// Information about an available model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Size in bytes (0 if unknown)
    pub size: u64,
    /// Estimated parameter count in billions (0 if unknown)
    pub param_billions: f64,
}

impl std::fmt::Display for ModelInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.size > 0 {
            let size_gb = self.size as f64 / (1024.0 * 1024.0 * 1024.0);
            write!(f, "{} ({:.1} GB)", self.name, size_gb)
        } else if self.param_billions > 0.0 {
            write!(f, "{} (~{:.0}B params)", self.name, self.param_billions)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// Create a provider from a ProviderConfig
pub fn from_config(name: &str, pcfg: &ProviderConfig) -> Result<Box<dyn Provider>> {
    match pcfg.kind.as_str() {
        "ollama" => {
            let base_url = if pcfg.base_url.is_empty() {
                "http://127.0.0.1:11434"
            } else {
                &pcfg.base_url
            };
            Ok(Box::new(ollama::OllamaProvider::new(base_url, &pcfg.model)?))
        }
        "openai_compat" => {
            Ok(Box::new(openai_compat::OpenAICompatProvider::new(
                name,
                &pcfg.api_key,
                &pcfg.base_url,
                &pcfg.model,
            )))
        }
        "anthropic" => {
            Ok(Box::new(claude::ClaudeProvider::new(
                &pcfg.api_key,
                &pcfg.model,
            )))
        }
        "" => bail!(
            "Provider '{}' has no kind set.\nRun 'niko settings configure' to set it up.",
            name
        ),
        other => bail!(
            "Unknown provider kind: '{}'\nSupported: ollama, openai_compat, anthropic",
            other
        ),
    }
}

/// Get the currently active provider
pub fn get_active_provider() -> Result<Box<dyn Provider>> {
    let (name, pcfg) = config::active_provider()?;
    from_config(&name, &pcfg)
}

/// Get a specific provider by name (with optional override)
pub fn get_provider(override_name: Option<&str>) -> Result<Box<dyn Provider>> {
    match override_name {
        Some(name) => {
            let cfg = config::load()?;
            let pcfg = cfg.providers.get(name)
                .ok_or_else(|| anyhow::anyhow!(
                    "Provider '{}' not configured.\nRun 'niko settings configure' to add it.",
                    name
                ))?;
            from_config(name, pcfg)
        }
        None => get_active_provider(),
    }
}

/// Estimate parameter count from model name or size
pub fn estimate_param_billions(model_name: &str, size_bytes: u64) -> f64 {
    // Try to parse from model name (common patterns: "7b", "14b", "32b", "70b")
    let lower = model_name.to_lowercase();
    for token in lower.split(&[':', '-', '_', '.'][..]) {
        if let Some(num_str) = token.strip_suffix('b') {
            if let Ok(n) = num_str.parse::<f64>() {
                return n;
            }
        }
    }

    // Fallback: estimate from file size (Q4 quantisation ~ 0.5 bytes/param)
    if size_bytes > 0 {
        return (size_bytes as f64) / (0.5 * 1_000_000_000.0);
    }

    0.0
}

/// Check if a model fits in the available RAM
pub fn model_fits_in_ram(param_billions: f64) -> bool {
    if param_billions <= 0.0 {
        return true; // can't tell, allow it
    }
    let max = config::max_model_size_for_ram() as f64;
    param_billions <= max
}
