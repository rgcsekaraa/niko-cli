pub mod ollama;
pub mod openai_compat;
pub mod claude;

use std::time::Duration;
use std::thread;

use anyhow::{Result, bail};

use crate::config::{self, ProviderConfig};

// ─── Retry configuration ────────────────────────────────────────────────────

/// Maximum number of retry attempts for LLM calls
const MAX_RETRIES: u32 = 3;
/// Base delay between retries (exponential backoff: base * 2^attempt)
const RETRY_BASE_DELAY_MS: u64 = 500;
/// Maximum delay cap
const RETRY_MAX_DELAY_MS: u64 = 8000;

/// Trait for all LLM providers
pub trait Provider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Generate a response from the LLM (single attempt — use generate_with_retry for production)
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

// ─── Retry wrapper ──────────────────────────────────────────────────────────

/// Determine if an error is retryable (transient network/server issues)
fn is_retryable_error(err: &anyhow::Error) -> bool {
    let msg = format!("{:#}", err).to_lowercase();

    // Network / connection errors
    if msg.contains("connection")
        || msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("reset by peer")
        || msg.contains("broken pipe")
        || msg.contains("eof")
        || msg.contains("dns")
        || msg.contains("resolve")
    {
        return true;
    }

    // HTTP 5xx server errors
    if msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("internal server error")
        || msg.contains("bad gateway")
        || msg.contains("service unavailable")
        || msg.contains("gateway timeout")
    {
        return true;
    }

    // Rate limiting (429)
    if msg.contains("429") || msg.contains("rate limit") || msg.contains("too many requests") {
        return true;
    }

    // Ollama-specific transient failures
    if msg.contains("model is loading") || msg.contains("model loading") {
        return true;
    }

    false
}

/// Generate with exponential backoff retry for transient failures
pub fn generate_with_retry(
    provider: &dyn Provider,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        match provider.generate(system_prompt, user_prompt) {
            Ok(response) => {
                // Validate the response isn't empty or garbage
                let trimmed = response.trim();
                if trimmed.is_empty() {
                    if attempt < MAX_RETRIES {
                        let delay = retry_delay(attempt);
                        eprintln!(
                            "  {} Empty response, retrying in {:.1}s... ({}/{})",
                            "↻".to_string(),
                            delay.as_secs_f64(),
                            attempt + 1,
                            MAX_RETRIES
                        );
                        thread::sleep(delay);
                        continue;
                    }
                    bail!("Provider returned empty response after {} attempts", MAX_RETRIES + 1);
                }
                return Ok(trimmed.to_string());
            }
            Err(e) => {
                if attempt < MAX_RETRIES && is_retryable_error(&e) {
                    let delay = retry_delay(attempt);
                    eprintln!(
                        "  {} {}, retrying in {:.1}s... ({}/{})",
                        "↻".to_string(),
                        summarize_error(&e),
                        delay.as_secs_f64(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    thread::sleep(delay);
                    last_err = Some(e);
                } else {
                    return Err(e);
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("All retry attempts exhausted")))
}

/// Calculate exponential backoff delay with jitter
fn retry_delay(attempt: u32) -> Duration {
    let base_ms = RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
    let delay_ms = base_ms.min(RETRY_MAX_DELAY_MS);
    // Add ~10% jitter to avoid thundering herd
    let jitter = delay_ms / 10;
    Duration::from_millis(delay_ms + jitter)
}

/// Create a short summary of an error for display
fn summarize_error(err: &anyhow::Error) -> String {
    let full = format!("{:#}", err);
    // Truncate long error messages
    if full.len() > 80 {
        format!("{}…", &full[..77])
    } else {
        full
    }
}

// ─── Provider factory ───────────────────────────────────────────────────────

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

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Estimate parameter count from model name or size
pub fn estimate_param_billions(model_name: &str, size_bytes: u64) -> f64 {
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
        return true;
    }
    let max = config::max_model_size_for_ram() as f64;
    param_billions <= max
}
