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
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
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

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Claude API error ({}): {}", status, text);
        }

        let msg: MessagesResponse = resp.json().context("Failed to parse Claude response")?;

        let content = msg.content
            .and_then(|blocks| blocks.into_iter().filter_map(|b| b.text).next())
            .unwrap_or_default();

        Ok(content.trim().to_string())
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
                // Fallback: the models endpoint may not be available for all accounts
                // Return known Claude models
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
