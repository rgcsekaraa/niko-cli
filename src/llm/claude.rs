use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::llm::{estimate_param_billions, ModelInfo, Provider};

/// Anthropic Claude Messages API provider with SSE streaming
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

/// SSE streaming event data
#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<StreamDelta>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(rename = "type", default)]
    delta_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
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

    fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            bail!(
                "API key not configured for Claude.\nRun 'niko settings configure' to set it up."
            );
        }
        if self.model.is_empty() {
            bail!(
                "No model selected for Claude.\nRun 'niko settings configure' to select a model."
            );
        }
        Ok(())
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn generate(&self, system_prompt: &str, user_prompt: &str, max_tokens: u32) -> Result<String> {
        self.validate()?;

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system_prompt,
            "messages": [{ "role": "user", "content": user_prompt }],
            "temperature": 0.1,
        });

        let resp = self
            .client
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
            if let Ok(err_resp) = serde_json::from_str::<ErrorResponse>(&text) {
                if let Some(err) = err_resp.error {
                    let err_type = err.error_type.unwrap_or_default();
                    let msg = err.message.unwrap_or_default();
                    bail!(
                        "Claude API error ({} {}): {}",
                        status.as_u16(),
                        err_type,
                        msg
                    );
                }
            }
            bail!("Claude API error ({}): {}", status.as_u16(), text);
        }

        let msg: MessagesResponse = resp.json().context("Failed to parse Claude response")?;

        if let Some(err) = msg.error {
            if let Some(emsg) = err.message {
                bail!("Claude API error: {}", emsg);
            }
        }

        if msg.stop_reason.as_deref() == Some("max_tokens") {
            eprintln!("  ⚠ Response truncated (hit max_tokens)");
        }

        let content = msg
            .content
            .map(|blocks| {
                blocks
                    .into_iter()
                    .filter_map(|b| b.text)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        let trimmed = content.trim();
        if trimmed.is_empty() {
            bail!("Claude returned empty response");
        }

        Ok(trimmed.to_string())
    }

    fn generate_stream(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        self.validate()?;

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system_prompt,
            "messages": [{ "role": "user", "content": user_prompt }],
            "temperature": 0.1,
            "stream": true,
        });

        let resp = self
            .client
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
            bail!("Claude API error ({}): {}", status.as_u16(), text);
        }

        let reader = BufReader::new(resp);
        let mut accumulated = String::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    if accumulated.is_empty() {
                        bail!("Stream read error: {}", e);
                    }
                    break;
                }
            };

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            // SSE: "data: {json}"
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                    match event.event_type.as_str() {
                        "content_block_delta" => {
                            if let Some(delta) = event.delta {
                                if delta.delta_type.as_deref() == Some("text_delta") {
                                    if let Some(text) = delta.text {
                                        if !text.is_empty() {
                                            on_token(&text);
                                            accumulated.push_str(&text);
                                        }
                                    }
                                }
                            }
                        }
                        "message_delta" => {
                            if let Some(delta) = event.delta {
                                if delta.stop_reason.as_deref() == Some("max_tokens") {
                                    eprintln!("\n  ⚠ Response truncated (hit max_tokens)");
                                }
                            }
                        }
                        "message_stop" => break,
                        _ => {} // Skip ping, message_start, content_block_start, etc.
                    }
                }
            }
        }

        if accumulated.trim().is_empty() {
            bail!("Claude returned empty streaming response");
        }

        Ok(accumulated.trim().to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if self.api_key.is_empty() {
            bail!("API key required to list Claude models.\nRun 'niko settings configure' to set it up.");
        }

        let resp = self
            .client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .timeout(Duration::from_secs(15))
            .send();

        match resp {
            Ok(r) if r.status().is_success() => {
                let list: ModelsListResponse =
                    r.json().context("Failed to parse Claude models response")?;

                Ok(list
                    .data
                    .unwrap_or_default()
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
                    .collect())
            }
            _ => Ok(vec![
                ModelInfo {
                    id: "claude-sonnet-4-20250514".into(),
                    name: "Claude Sonnet 4".into(),
                    size: 0,
                    param_billions: 0.0,
                },
                ModelInfo {
                    id: "claude-3-5-haiku-20241022".into(),
                    name: "Claude 3.5 Haiku".into(),
                    size: 0,
                    param_billions: 0.0,
                },
                ModelInfo {
                    id: "claude-3-5-sonnet-20241022".into(),
                    name: "Claude 3.5 Sonnet".into(),
                    size: 0,
                    param_billions: 0.0,
                },
                ModelInfo {
                    id: "claude-3-opus-20240229".into(),
                    name: "Claude 3 Opus".into(),
                    size: 0,
                    param_billions: 0.0,
                },
            ]),
        }
    }
}
