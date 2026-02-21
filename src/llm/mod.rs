pub mod claude;
pub mod ollama;
pub mod openai_compat;

use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::config::{self, ProviderConfig};

// ─── Retry configuration ────────────────────────────────────────────────────

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;
const RETRY_MAX_DELAY_MS: u64 = 8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Trait for all LLM providers
pub trait Provider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Generate a response (non-streaming)
    fn generate(&self, messages: &[Message], max_tokens: u32) -> Result<String>;

    /// Stream tokens to a callback, return accumulated response.
    /// Default: falls back to non-streaming generate.
    fn generate_stream(
        &self,
        messages: &[Message],
        max_tokens: u32,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        let result = self.generate(messages, max_tokens)?;
        on_token(&result);
        Ok(result)
    }

    /// Check if the provider is available
    fn is_available(&self) -> bool;

    /// Fetch all available models from this provider
    fn list_models(&self) -> Result<Vec<ModelInfo>>;
}

/// Information about an available model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
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

// ─── Retry wrapper (non-streaming) ──────────────────────────────────────────

fn is_retryable_error(err: &anyhow::Error) -> bool {
    let msg = format!("{:#}", err).to_lowercase();
    msg.contains("connection")
        || msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("reset by peer")
        || msg.contains("broken pipe")
        || msg.contains("eof")
        || msg.contains("dns")
        || msg.contains("resolve")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("429")
        || msg.contains("rate limit")
        || msg.contains("too many requests")
        || msg.contains("model is loading")
        || msg.contains("model loading")
}

/// Non-streaming generate with retry — used for cmd mode and synthesis steps
pub fn generate_with_retry(
    provider: &dyn Provider,
    messages: &[Message],
    max_tokens: u32,
) -> Result<String> {
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        match provider.generate(messages, max_tokens) {
            Ok(response) => {
                let trimmed = response.trim();
                if trimmed.is_empty() {
                    if attempt < MAX_RETRIES {
                        let delay = retry_delay(attempt);
                        eprintln!(
                            "  ↻ Empty response, retrying in {:.1}s… ({}/{})",
                            delay.as_secs_f64(),
                            attempt + 1,
                            MAX_RETRIES
                        );
                        thread::sleep(delay);
                        continue;
                    }
                    bail!(
                        "Provider returned empty response after {} attempts",
                        MAX_RETRIES + 1
                    );
                }
                return Ok(trimmed.to_string());
            }
            Err(e) => {
                if attempt < MAX_RETRIES && is_retryable_error(&e) {
                    let delay = retry_delay(attempt);
                    eprintln!(
                        "  ↻ {}, retrying in {:.1}s… ({}/{})",
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

/// Streaming generate — no retry once tokens start flowing.
/// Retries only on initial connection failure (before any tokens arrive).
pub fn generate_streaming(
    provider: &dyn Provider,
    messages: &[Message],
    max_tokens: u32,
    on_token: &mut dyn FnMut(&str),
) -> Result<String> {
    // Try once; if connection fails before any tokens, retry with non-streaming
    match provider.generate_stream(messages, max_tokens, on_token) {
        Ok(result) => {
            let trimmed = result.trim();
            if trimmed.is_empty() {
                bail!("Provider returned empty response");
            }
            Ok(trimmed.to_string())
        }
        Err(e) => {
            if is_retryable_error(&e) {
                eprintln!("  ↻ Stream failed, retrying without streaming…");
                // Fallback to non-streaming with retry
                generate_with_retry(provider, messages, max_tokens)
            } else {
                Err(e)
            }
        }
    }
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
    let delay_ms = base_ms.min(RETRY_MAX_DELAY_MS);
    let jitter = delay_ms / 10;
    Duration::from_millis(delay_ms + jitter)
}

fn summarize_error(err: &anyhow::Error) -> String {
    let full = format!("{:#}", err);
    if full.len() > 80 {
        // Find a safe char boundary at or before 77
        let mut end = 77;
        while end > 0 && !full.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &full[..end])
    } else {
        full
    }
}

// ─── Provider factory ───────────────────────────────────────────────────────

pub fn from_config(name: &str, pcfg: &ProviderConfig) -> Result<Box<dyn Provider>> {
    match pcfg.kind.as_str() {
        "ollama" => {
            let base_url = if pcfg.base_url.is_empty() {
                "http://127.0.0.1:11434"
            } else {
                &pcfg.base_url
            };
            Ok(Box::new(ollama::OllamaProvider::new(
                base_url,
                &pcfg.model,
                pcfg.options.clone(),
            )?))
        }
        "openai_compat" => Ok(Box::new(openai_compat::OpenAICompatProvider::new(
            name,
            &pcfg.api_key,
            &pcfg.base_url,
            &pcfg.model,
        ))),
        "anthropic" => Ok(Box::new(claude::ClaudeProvider::new(
            &pcfg.api_key,
            &pcfg.model,
        ))),
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

pub fn get_active_provider() -> Result<Box<dyn Provider>> {
    let (name, pcfg) = config::active_provider()?;
    from_config(&name, &pcfg)
}

pub fn get_provider(override_name: Option<&str>) -> Result<Box<dyn Provider>> {
    match override_name {
        Some(name) => {
            let cfg = config::load()?;
            let pcfg = cfg.providers.get(name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Provider '{}' not configured.\nRun 'niko settings configure' to add it.",
                    name
                )
            })?;
            from_config(name, pcfg)
        }
        None => get_active_provider(),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

pub fn estimate_param_billions(model_name: &str, size_bytes: u64) -> f64 {
    let lower = model_name.to_lowercase();
    for token in lower.split(&[':', '-', '_', '.'][..]) {
        if let Some(num_str) = token.strip_suffix('b') {
            if let Ok(n) = num_str.parse::<f64>() {
                return n;
            }
        }
    }
    if size_bytes > 0 {
        return (size_bytes as f64) / (0.5 * 1_000_000_000.0);
    }
    0.0
}

pub fn model_fits_in_ram(param_billions: f64) -> bool {
    if param_billions <= 0.0 {
        return true;
    }
    let max = config::max_model_size_for_ram() as f64;
    param_billions <= max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_params_from_model_name_tokens() {
        assert_eq!(estimate_param_billions("qwen2.5-coder:7b", 0), 7.0);
        assert_eq!(estimate_param_billions("llama3.2_1b", 0), 1.0);
        assert_eq!(estimate_param_billions("model-14b-instruct", 0), 14.0);
    }

    #[test]
    fn estimate_params_from_model_size_fallback() {
        let one_b_params_q4_bytes = 500_000_000_u64;
        let estimated = estimate_param_billions("unknown-model", one_b_params_q4_bytes);
        assert!((estimated - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn non_positive_model_size_is_always_allowed() {
        assert!(model_fits_in_ram(0.0));
        assert!(model_fits_in_ram(-1.0));
    }
}
