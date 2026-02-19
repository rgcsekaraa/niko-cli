use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::llm::{Provider, ModelInfo, estimate_param_billions};

/// OpenAI-compatible provider with SSE streaming support
pub struct OpenAICompatProvider {
    provider_name: String,
    api_key: String,
    base_url: String,
    model: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Option<Vec<Choice>>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

/// SSE streaming chunk
#[derive(Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChoice>>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct ApiError {
    message: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Option<Vec<ApiModel>>,
}

#[derive(Deserialize)]
struct ApiModel {
    id: String,
}

impl OpenAICompatProvider {
    pub fn new(provider_name: &str, api_key: &str, base_url: &str, model: &str) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            provider_name: provider_name.to_string(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            bail!(
                "API key not configured for '{}'.\nRun 'niko settings configure' to set it up.",
                self.provider_name
            );
        }
        if self.model.is_empty() {
            bail!(
                "No model selected for '{}'.\nRun 'niko settings configure' to select a model.",
                self.provider_name
            );
        }
        Ok(())
    }
}

impl Provider for OpenAICompatProvider {
    fn name(&self) -> &str { &self.provider_name }

    fn is_available(&self) -> bool { !self.api_key.is_empty() }

    fn generate(&self, system_prompt: &str, user_prompt: &str, max_tokens: u32) -> Result<String> {
        self.validate()?;

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_tokens": max_tokens,
        });

        let resp = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("Failed to call {} API", self.provider_name))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("{} API error ({}): {}", self.provider_name, status.as_u16(), text);
        }

        let completion: ChatCompletionResponse = resp.json()
            .with_context(|| format!("Failed to parse {} response", self.provider_name))?;

        if let Some(err) = completion.error {
            if let Some(msg) = err.message {
                bail!("{} API error: {}", self.provider_name, msg);
            }
        }

        let choice = completion.choices.and_then(|c| c.into_iter().next());

        let content = match choice {
            Some(c) => {
                if c.finish_reason.as_deref() == Some("length") {
                    eprintln!("  ⚠ Response truncated (hit max_tokens)");
                }
                c.message.content.unwrap_or_default()
            }
            None => bail!("{} returned no choices", self.provider_name),
        };

        let trimmed = content.trim();
        if trimmed.is_empty() {
            bail!("{} returned empty response", self.provider_name);
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
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_tokens": max_tokens,
            "stream": true,
        });

        let resp = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("Failed to call {} API", self.provider_name))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("{} API error ({}): {}", self.provider_name, status.as_u16(), text);
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
            if line.is_empty() { continue; }

            // SSE format: "data: {json}" or "data: [DONE]"
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" { break; }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(choices) = chunk.choices {
                        for choice in choices {
                            if let Some(delta) = choice.delta {
                                if let Some(content) = delta.content {
                                    if !content.is_empty() {
                                        on_token(&content);
                                        accumulated.push_str(&content);
                                    }
                                }
                            }
                            if choice.finish_reason.as_deref() == Some("length") {
                                eprintln!("\n  ⚠ Response truncated (hit max_tokens)");
                            }
                        }
                    }
                }
            }
        }

        if accumulated.trim().is_empty() {
            bail!("{} returned empty streaming response", self.provider_name);
        }

        Ok(accumulated.trim().to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if self.api_key.is_empty() {
            bail!(
                "API key required to list models for '{}'.\nRun 'niko settings configure' to set it up.",
                self.provider_name
            );
        }

        let resp = self.client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(Duration::from_secs(15))
            .send()
            .with_context(|| format!("Failed to fetch models from {}", self.provider_name))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Failed to list models ({}): {}", status, text);
        }

        let models_resp: ModelsResponse = resp.json()
            .with_context(|| "Failed to parse models response")?;

        Ok(models_resp.data.unwrap_or_default()
            .into_iter()
            .map(|m| {
                let params = estimate_param_billions(&m.id, 0);
                ModelInfo { name: m.id.clone(), id: m.id, size: 0, param_billions: params }
            })
            .collect())
    }
}
