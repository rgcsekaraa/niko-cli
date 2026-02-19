use std::io::{BufRead, BufReader};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::llm::{Provider, ModelInfo, estimate_param_billions};

pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// Streaming response — one JSON object per line
#[derive(Deserialize)]
struct StreamChunk {
    message: Option<StreamMessage>,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct StreamMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Option<Vec<OllamaModel>>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct PullProgress {
    status: Option<String>,
    completed: Option<u64>,
    total: Option<u64>,
}

impl OllamaProvider {
    pub fn new(base_url: &str, model: &str) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        })
    }

    fn is_server_running(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn has_model(&self, model: &str) -> bool {
        if model.is_empty() { return false; }
        match self.fetch_local_models() {
            Ok(models) => models.iter().any(|m| m.id == model || m.id.starts_with(model)),
            Err(_) => false,
        }
    }

    fn fetch_local_models(&self) -> Result<Vec<ModelInfo>> {
        let resp = self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .context("Failed to connect to Ollama")?;

        if !resp.status().is_success() {
            bail!("Ollama API returned status: {}", resp.status());
        }

        let tags: TagsResponse = resp.json().context("Failed to parse Ollama response")?;

        Ok(tags.models.unwrap_or_default().into_iter().map(|m| {
            let param_b = estimate_param_billions(&m.name, m.size);
            ModelInfo {
                id: m.name.clone(),
                name: m.name,
                size: m.size,
                param_billions: param_b,
            }
        }).collect())
    }

    pub fn pull_model(&self, model: &str) -> Result<()> {
        eprintln!("  Downloading '{}'...", model);

        let body = serde_json::json!({ "name": model, "stream": true });
        let resp = self.client
            .post(format!("{}/api/pull", self.base_url))
            .json(&body)
            .timeout(Duration::from_secs(7200))
            .send()
            .context("Failed to start model download")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Ollama pull failed ({}): {}", status, text);
        }

        let reader = BufReader::new(resp);
        let mut last_status = String::new();

        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => continue };
            if line.trim().is_empty() { continue; }
            if let Ok(p) = serde_json::from_str::<PullProgress>(&line) {
                let status = p.status.unwrap_or_default();
                if let (Some(done), Some(total)) = (p.completed, p.total) {
                    if total > 0 {
                        let pct = (done as f64 / total as f64) * 100.0;
                        eprint!("\r  {}: {:.1}%   ", status, pct);
                    }
                } else if status != last_status {
                    eprint!("\r  {}          ", status);
                }
                last_status = status;
            }
        }
        eprintln!();
        Ok(())
    }

    fn ensure_model_available(&self) -> Result<()> {
        if self.model.is_empty() {
            bail!(
                "No model selected for Ollama.\n\
                 Run 'niko settings configure' to select a model."
            );
        }

        if !self.has_model(&self.model) {
            eprintln!("  Model '{}' not found locally, pulling...", self.model);
            self.pull_model(&self.model)?;
        }

        Ok(())
    }

    /// Build the request body with performance optimizations
    fn build_request_body(&self, system_prompt: &str, user_prompt: &str, max_tokens: u32, stream: bool) -> serde_json::Value {
        // Adaptive context window based on input size
        let total_chars = system_prompt.len() + user_prompt.len();
        let num_ctx = if total_chars > 50_000 {
            16384
        } else if total_chars > 20_000 {
            8192
        } else {
            4096
        };

        serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": stream,
            "keep_alive": "30m",
            "options": {
                "temperature": 0.1,
                "num_predict": max_tokens,
                "num_ctx": num_ctx,
                "top_p": 0.7,
                "top_k": 20,
                "repeat_penalty": 1.2,
                "flash_attn": true
            }
        })
    }
}

impl Provider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    fn is_available(&self) -> bool {
        self.is_server_running()
    }

    fn generate(&self, system_prompt: &str, user_prompt: &str, max_tokens: u32) -> Result<String> {
        // No pre-check — just attempt the request, handle errors directly
        let body = self.build_request_body(system_prompt, user_prompt, max_tokens, false);

        // Ensure model is pulled (only checks on first call, then server has it cached)
        self.ensure_model_available().map_err(|e| {
            if format!("{:#}", e).contains("connect") {
                anyhow::anyhow!(
                    "Ollama is not running at {}.\n\
                     Start it with: ollama serve\n\
                     Install from:  https://ollama.com/download",
                    self.base_url
                )
            } else {
                e
            }
        })?;

        let resp = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    anyhow::anyhow!(
                        "Ollama is not running at {}.\n\
                         Start it with: ollama serve",
                        self.base_url
                    )
                } else {
                    anyhow::anyhow!("Failed to call Ollama: {}", e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Ollama error ({}): {}", status, text);
        }

        let chat: ChatResponse = resp.json().context("Failed to parse Ollama response")?;
        let content = chat.message.map(|m| m.content).unwrap_or_default();
        let trimmed = content.trim();

        if trimmed.is_empty() {
            bail!("Ollama returned empty response");
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
        self.ensure_model_available().map_err(|e| {
            if format!("{:#}", e).contains("connect") {
                anyhow::anyhow!(
                    "Ollama is not running at {}.\nStart it with: ollama serve",
                    self.base_url
                )
            } else {
                e
            }
        })?;

        let body = self.build_request_body(system_prompt, user_prompt, max_tokens, true);

        let resp = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    anyhow::anyhow!(
                        "Ollama is not running at {}.\nStart it with: ollama serve",
                        self.base_url
                    )
                } else {
                    anyhow::anyhow!("Failed to call Ollama: {}", e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Ollama error ({}): {}", status, text);
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
                    break; // Return what we have
                }
            };

            if line.trim().is_empty() { continue; }

            match serde_json::from_str::<StreamChunk>(&line) {
                Ok(chunk) => {
                    if let Some(msg) = chunk.message {
                        if !msg.content.is_empty() {
                            on_token(&msg.content);
                            accumulated.push_str(&msg.content);
                        }
                    }
                    if chunk.done { break; }
                }
                Err(_) => continue, // Skip malformed lines
            }
        }

        if accumulated.trim().is_empty() {
            bail!("Ollama returned empty streaming response");
        }

        Ok(accumulated.trim().to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if !self.is_server_running() {
            bail!("Ollama is not running. Start it with: ollama serve");
        }
        self.fetch_local_models()
    }
}

// ─── Installation helpers ───────────────────────────────────────────────────

pub fn is_ollama_installed() -> bool {
    Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn is_ollama_running() -> bool {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .connect_timeout(Duration::from_secs(1))
        .build()
        .ok()
        .and_then(|c| {
            c.get("http://127.0.0.1:11434/api/tags")
                .send().ok()
                .map(|r| r.status().is_success())
        })
        .unwrap_or(false)
}

pub fn install_ollama() -> Result<()> {
    eprintln!("  Installing Ollama...");
    if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
        let status = Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .status()
            .context("Failed to run Ollama installer")?;
        if !status.success() {
            bail!("Ollama installation failed.\nInstall manually from: https://ollama.com/download");
        }
    } else if cfg!(target_os = "windows") {
        let status = Command::new("powershell")
            .args(["-Command",
                "Invoke-WebRequest -Uri 'https://ollama.com/download/OllamaSetup.exe' -OutFile '$env:TEMP\\OllamaSetup.exe'; Start-Process '$env:TEMP\\OllamaSetup.exe' -Wait"
            ])
            .status()
            .context("Failed to download Ollama installer")?;
        if !status.success() {
            bail!("Ollama installation failed.\nDownload manually from: https://ollama.com/download");
        }
    } else {
        bail!("Unsupported OS. Install Ollama manually from: https://ollama.com/download");
    }
    eprintln!("  ✓ Ollama installed successfully");
    Ok(())
}

pub fn search_ollama_models(query: &str) -> Result<Vec<ModelInfo>> {
    let known_models = vec![
        ("qwen2.5-coder:0.5b", 0.5), ("qwen2.5-coder:1.5b", 1.5),
        ("qwen2.5-coder:3b", 3.0), ("qwen2.5-coder:7b", 7.0),
        ("qwen2.5-coder:14b", 14.0), ("qwen2.5-coder:32b", 32.0),
        ("deepseek-coder-v2:16b", 16.0),
        ("codellama:7b", 7.0), ("codellama:13b", 13.0), ("codellama:34b", 34.0),
        ("starcoder2:3b", 3.0), ("starcoder2:7b", 7.0), ("starcoder2:15b", 15.0),
        ("llama3.2:1b", 1.0), ("llama3.2:3b", 3.0),
        ("llama3.1:8b", 8.0), ("llama3.1:70b", 70.0),
        ("gemma2:2b", 2.0), ("gemma2:9b", 9.0), ("gemma2:27b", 27.0),
        ("mistral:7b", 7.0), ("mixtral:8x7b", 47.0),
        ("phi3:3.8b", 3.8), ("phi3:14b", 14.0),
        ("deepseek-r1:1.5b", 1.5), ("deepseek-r1:7b", 7.0), ("deepseek-r1:8b", 8.0),
        ("deepseek-r1:14b", 14.0), ("deepseek-r1:32b", 32.0), ("deepseek-r1:70b", 70.0),
    ];

    let query_lower = query.to_lowercase();
    let max_params = crate::config::max_model_size_for_ram() as f64;

    Ok(known_models.into_iter()
        .filter(|(name, _)| query.is_empty() || name.contains(&query_lower))
        .filter(|(_, params)| *params <= max_params)
        .map(|(name, params)| ModelInfo {
            id: name.to_string(), name: name.to_string(),
            size: 0, param_billions: params,
        })
        .collect())
}
