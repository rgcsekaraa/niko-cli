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
            .timeout(Duration::from_secs(3))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn has_model(&self, model: &str) -> bool {
        if model.is_empty() {
            return false;
        }
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

    /// Pull a model with progress
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
            bail!("Ollama pull failed: {}", resp.status());
        }

        let reader = BufReader::new(resp);
        let mut last_status = String::new();

        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => continue };
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
                 Run 'niko settings configure' to select a model,\n\
                 or 'niko settings set ollama model <name>' to set one."
            );
        }

        if !self.has_model(&self.model) {
            eprintln!("  Model '{}' not found locally, pulling...", self.model);
            self.pull_model(&self.model)?;
        }

        Ok(())
    }
}

impl Provider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    fn is_available(&self) -> bool {
        self.is_server_running()
    }

    fn generate(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        if !self.is_server_running() {
            bail!(
                "Ollama is not running at {}.\n\
                 Start it with: ollama serve\n\
                 Install from:  https://ollama.com/download",
                self.base_url
            );
        }

        self.ensure_model_available()?;

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": false,
            "options": {
                "temperature": 0.1,
                "num_predict": 4096,
                "top_p": 0.7,
                "top_k": 20,
                "repeat_penalty": 1.2
            }
        });

        let resp = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .context("Failed to call Ollama")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("Ollama error ({}): {}", status, text);
        }

        let chat: ChatResponse = resp.json().context("Failed to parse response")?;
        let content = chat.message.map(|m| m.content).unwrap_or_default();
        Ok(content.trim().to_string())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if !self.is_server_running() {
            bail!("Ollama is not running. Start it with: ollama serve");
        }
        self.fetch_local_models()
    }
}

// ─── Ollama installation helpers ────────────────────────────────────────────

/// Check if Ollama is installed on this system
pub fn is_ollama_installed() -> bool {
    Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if Ollama server is reachable
pub fn is_ollama_running() -> bool {
    reqwest::blocking::Client::new()
        .get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_secs(2))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Install Ollama (cross-platform)
pub fn install_ollama() -> Result<()> {
    eprintln!("  Installing Ollama...");

    if cfg!(target_os = "macos") {
        // macOS: use the official install script
        let status = Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .status()
            .context("Failed to run Ollama installer")?;

        if !status.success() {
            bail!(
                "Ollama installation failed.\n\
                 Install manually from: https://ollama.com/download"
            );
        }
    } else if cfg!(target_os = "linux") {
        let status = Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .status()
            .context("Failed to run Ollama installer")?;

        if !status.success() {
            bail!(
                "Ollama installation failed.\n\
                 Install manually from: https://ollama.com/download"
            );
        }
    } else if cfg!(target_os = "windows") {
        // Windows: download and run the installer via PowerShell
        let status = Command::new("powershell")
            .args([
                "-Command",
                "Invoke-WebRequest -Uri 'https://ollama.com/download/OllamaSetup.exe' -OutFile '$env:TEMP\\OllamaSetup.exe'; Start-Process '$env:TEMP\\OllamaSetup.exe' -Wait"
            ])
            .status()
            .context("Failed to download Ollama installer")?;

        if !status.success() {
            bail!(
                "Ollama installation failed.\n\
                 Download manually from: https://ollama.com/download"
            );
        }
    } else {
        bail!("Unsupported OS. Install Ollama manually from: https://ollama.com/download");
    }

    eprintln!("  ✓ Ollama installed successfully");
    Ok(())
}

/// Search for models in the Ollama library (uses the search endpoint)
pub fn search_ollama_models(query: &str) -> Result<Vec<ModelInfo>> {
    // Ollama doesn't have a public search API, so we provide well-known coding models
    // and filter by query
    let known_models = vec![
        ("qwen2.5-coder:0.5b", 0.5),
        ("qwen2.5-coder:1.5b", 1.5),
        ("qwen2.5-coder:3b", 3.0),
        ("qwen2.5-coder:7b", 7.0),
        ("qwen2.5-coder:14b", 14.0),
        ("qwen2.5-coder:32b", 32.0),
        ("deepseek-coder-v2:16b", 16.0),
        ("codellama:7b", 7.0),
        ("codellama:13b", 13.0),
        ("codellama:34b", 34.0),
        ("starcoder2:3b", 3.0),
        ("starcoder2:7b", 7.0),
        ("starcoder2:15b", 15.0),
        ("llama3.2:1b", 1.0),
        ("llama3.2:3b", 3.0),
        ("llama3.1:8b", 8.0),
        ("llama3.1:70b", 70.0),
        ("gemma2:2b", 2.0),
        ("gemma2:9b", 9.0),
        ("gemma2:27b", 27.0),
        ("mistral:7b", 7.0),
        ("mixtral:8x7b", 47.0),
        ("phi3:3.8b", 3.8),
        ("phi3:14b", 14.0),
        ("deepseek-r1:1.5b", 1.5),
        ("deepseek-r1:7b", 7.0),
        ("deepseek-r1:8b", 8.0),
        ("deepseek-r1:14b", 14.0),
        ("deepseek-r1:32b", 32.0),
        ("deepseek-r1:70b", 70.0),
    ];

    let query_lower = query.to_lowercase();
    let max_params = crate::config::max_model_size_for_ram() as f64;

    let models: Vec<ModelInfo> = known_models
        .into_iter()
        .filter(|(name, _)| query.is_empty() || name.contains(&query_lower))
        .filter(|(_, params)| *params <= max_params)
        .map(|(name, params)| ModelInfo {
            id: name.to_string(),
            name: name.to_string(),
            size: 0,
            param_billions: params,
        })
        .collect();

    Ok(models)
}
