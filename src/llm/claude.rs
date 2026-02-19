use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::llm::{Provider, ModelInfo, estimate_param_billions};

/// Anthropic Claude Messages API provider
pub struct ClaudeProvider {
    api_key: String,
    model: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Option<Vec<ContentBlock>>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize, Default)]
struct ApiError {
    message: Option<String>,
    #[serde(rename = "type", default)]
    error_type: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ModelsListResponse {
    data: Option<Vec<ClaudeModel>>,
}

#[derive(Deserialize)]
struct ClaudeModel {
    id: String,
    #[serde(default)]
    display_name: String,
}

impl ClaudeProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        // Connection pool with keep-alive
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client,
        }
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str { "claude" }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn generate(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        if self.api_key.is_empty() {
            bail!(
                "API key not configured for Claude.\n\
                 Run 'niko settings configure' to set it up."
            );
        }

        if self.model.is_empty() {
            bail!(
                "No model selected for Claude.\n\
                 Run 'niko settings configure' to select a model."
            );
        }

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
        });

        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .context("Failed to call Claude API")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            // Try parsing structured error
            if let Ok(err_resp) = serde_json::from_str::<ErrorResponse>(&text) {
                if let Some(err) = err_resp.error {
                    let err_type = err.error_type.unwrap_or_default();
                    let msg = err.message.unwrap_or_default();
                    bail!("Claude API error ({} {}): {}", status.as_u16(), err_type, msg);
                }
            }
            bail!("Claude API error ({}): {}", status.as_u16(), text);
        }

        let msg: MessagesResponse = resp.json().context("Failed to parse Claude response")?;

        // Check embedded errors
        if let Some(err) = msg.error {
            if let Some(emsg) = err.message {
                bail!("Claude API error: {}", emsg);
            }
        }

        // Warn if truncated
        if msg.stop_reason.as_deref() == Some("max_tokens") {
            eprintln!("  {} Response was truncated (hit max_tokens)", "⚠".to_string());
        }

        let content = msg.content
            .and_then(|blocks| {
                blocks.into_iter()
                    .filter_map(|b| b.text)
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into()
            })
            .unwrap_or_default();

        let trimmed = content.trim();
        if trimmed.is_empty() {
            bail!("Claude returned empty response content");
        }

        Ok(trimmed.to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if self.api_key.is_empty() {
            bail!(
                "API key required to list Claude models.\n\
                 Run 'niko settings configure' to set it up."
            );
        }

        let resp = self.client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(Duration::from_secs(15))
            .send();

        match resp {
            Ok(r) if r.status().is_success() => {
                let list: ModelsListResponse = r.json()
                    .context("Failed to parse Claude models response")?;

                let models = list.data.unwrap_or_default()
                    .into_iter()
                    .map(|m| {
                        let display = if m.display_name.is_empty() {
                            m.id.clone()
                        } else {
                            m.display_name
                        };
                        let params = estimate_param_billions(&m.id, 0);
                        ModelInfo {
                            name: display,
                            id: m.id,
                            size: 0,
                            param_billions: params,
                        }
                    })
                    .collect();

                Ok(models)
            }
            _ => {
                // Fallback to known Claude models
                Ok(vec![
                    ModelInfo { id: "claude-sonnet-4-20250514".into(), name: "Claude Sonnet 4".into(), size: 0, param_billions: 0.0 },
                    ModelInfo { id: "claude-3-5-haiku-20241022".into(), name: "Claude 3.5 Haiku".into(), size: 0, param_billions: 0.0 },
                    ModelInfo { id: "claude-3-5-sonnet-20241022".into(), name: "Claude 3.5 Sonnet".into(), size: 0, param_billions: 0.0 },
                    ModelInfo { id: "claude-3-opus-20240229".into(), name: "Claude 3 Opus".into(), size: 0, param_billions: 0.0 },
                ])
            }
        }
    }
}
