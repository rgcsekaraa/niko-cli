use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::llm::{Provider, ModelInfo, estimate_param_billions};

/// OpenAI-compatible provider — works with OpenAI, DeepSeek, Grok, Groq, Together, Mistral, OpenRouter, etc.
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
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
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
}

impl Provider for OpenAICompatProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn generate(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        if self.api_key.is_empty() {
            bail!(
                "API key not configured for '{}'.\n\
                 Run 'niko settings configure' to set it up.",
                self.provider_name
            );
        }

        if self.model.is_empty() {
            bail!(
                "No model selected for '{}'.\n\
                 Run 'niko settings configure' to select a model.",
                self.provider_name
            );
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_tokens": 4096,
        });

        let resp = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .with_context(|| format!("Failed to call {} API", self.provider_name))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("{} API error ({}): {}", self.provider_name, status, text);
        }

        let completion: ChatCompletionResponse = resp.json()
            .with_context(|| format!("Failed to parse {} response", self.provider_name))?;

        let content = completion.choices
            .and_then(|c| c.into_iter().next())
            .map(|c| c.message.content)
            .unwrap_or_default();

        Ok(content.trim().to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if self.api_key.is_empty() {
            bail!(
                "API key required to list models for '{}'.\n\
                 Run 'niko settings configure' to set it up.",
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

        let models = models_resp.data.unwrap_or_default()
            .into_iter()
            .map(|m| {
                let params = estimate_param_billions(&m.id, 0);
                ModelInfo {
                    name: m.id.clone(),
                    id: m.id,
                    size: 0,
                    param_billions: params,
                }
            })
            .collect();

        Ok(models)
    }
}
