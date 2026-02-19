# Niko

**AI-powered CLI: explain code, generate shell commands, use any LLM provider.**

Built in Rust. Works on macOS, Linux, and Windows.

```bash
$ niko cmd "find all files larger than 100MB"
find . -type f -size +100M
Copied to clipboard

$ cat main.rs | niko explain
📖 42 lines analyzed — completed in 2.1s
## Overview
...
```

---

## Features

- **Three Modes** — `cmd`, `explain`, `settings`
- **Dynamic LLM Providers** — Any OpenAI-compatible API, Claude, or local Ollama
- **Dynamic Model Selection** — Fetches available models from the API, no hardcoded lists
- **RAM-Based Restrictions** — Prevents selecting models too large for your hardware
- **Auto-Install Ollama** — Installs Ollama automatically if not present
- **Smart Code Chunking** — Splits large files at function boundaries with context memory between chunks
- **Automatic Retry** — Exponential backoff for transient failures (timeouts, rate limits, 5xx errors)
- **Connection Pooling** — Keep-alive HTTP connections for fast sequential LLM calls
- **Command Generation** — Natural language → shell commands, auto-copied to clipboard
- **Safety Warnings** — Flags dangerous commands before execution
- **Cross-Platform** — macOS, Linux (Ubuntu/Debian/etc.), Windows

---

## Install

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/rgcsekaraa/niko-cli/main/scripts/install.sh | sh
```

### With Cargo

```bash
cargo install --git https://github.com/rgcsekaraa/niko-cli
```

### From Source

```bash
git clone https://github.com/rgcsekaraa/niko-cli.git
cd niko-cli
make install
```

---

## Quick Start

```bash
# First run — interactive setup wizard
niko settings configure
```

This will:
1. Show available providers (Ollama, OpenAI, Claude, DeepSeek, Grok, Groq, Mistral, Together, OpenRouter, or custom)
2. For **Ollama**: auto-install if needed → list local models → show downloadable models filtered by your RAM → let you pick
3. For **API providers**: ask for API key → fetch available models from the API → let you pick
4. Save everything to `~/.niko/config.yaml`

---

## Usage

### `cmd` — Generate Shell Commands

```bash
$ niko cmd "find python files modified today"
find . -name "*.py" -mtime 0
Copied to clipboard

$ niko cmd "kill process on port 3000"
$ niko cmd "compress logs folder to tar.gz"
$ niko cmd "git commits from last week"
$ niko cmd "show disk usage by directory"
```

### `explain` — Explain Code

```bash
# From a file
niko explain -f src/main.rs

# Pipe code in
cat complex_module.py | niko explain

# Paste interactively (live line counter, Ctrl-D or two empty lines to finish)
niko explain
```

For large files, Niko:
1. **Chunks** code at function/block boundaries (max 200 lines/chunk)
2. **Carries context** — each chunk includes overlapping lines and a running summary from previous chunks
3. **Retries** failed LLM calls with exponential backoff (3 attempts, 500ms → 4s delay)
4. **Synthesises** chunk analyses into an overall summary with follow-up questions

### `settings` — Configuration

```bash
# Interactive setup wizard
niko settings configure

# Show current config
niko settings show

# Set a value directly
niko settings set openai.api_key sk-xxx
niko settings set openai.model gpt-4o
niko settings set active_provider openai

# Reset to defaults
niko settings init

# Print config path
niko settings path
```

### Override Provider Per-Command

```bash
niko cmd "list files" --provider openai
niko explain -f main.rs --provider claude
```

---

## Reliability & Performance

Niko is designed for production use with reliability and speed:

| Feature | Details |
|---------|---------|
| **Streaming** | Tokens appear immediately as the LLM generates them (all providers) |
| **Retry** | 3 attempts with exponential backoff (500ms → 2s + jitter) |
| **Retryable errors** | Timeouts, connection resets, 429/5xx, rate limits, model loading |
| **Connection pooling** | HTTP keep-alive, 4 idle connections/host, TCP keepalive 30s |
| **Model keep-alive** | Ollama keeps model in VRAM for 30 min (no reload between calls) |
| **Flash attention** | Enabled by default for Ollama (faster on Apple Silicon / GPU) |
| **Adaptive tokens** | `cmd` mode uses 512 max tokens, `explain` uses 4096 — less KV cache for short tasks |
| **Adaptive context** | Ollama context window scales with prompt size (4K → 16K) |
| **Empty response guard** | Detects and retries empty/null LLM responses |
| **Truncation detection** | Warns when response hits max_tokens (Claude, OpenAI) |
| **Context memory** | Multi-chunk explanations carry 10-line code overlap for boundary continuity |
| **Structured errors** | Parses API error responses for clear, actionable messages |

---

## Supported Providers

| Provider | Type | How to set up |
|----------|------|---------------|
| **Ollama** | Local (free) | Auto-installed, models downloaded on demand |
| **OpenAI** | API | `niko settings configure` → select OpenAI → enter key |
| **Claude** | API | `niko settings configure` → select Claude → enter key |
| **DeepSeek** | API | `niko settings configure` → select DeepSeek → enter key |
| **Grok** | API | `niko settings configure` → select Grok → enter key |
| **Groq** | API | `niko settings configure` → select Groq → enter key |
| **Mistral** | API | `niko settings configure` → select Mistral → enter key |
| **Together** | API | `niko settings configure` → select Together → enter key |
| **OpenRouter** | API | `niko settings configure` → select OpenRouter → enter key |
| **Custom** | API | `niko settings configure` → choose "Custom" → enter URL + key |

All API providers fetch models dynamically from their `/models` endpoint — **nothing is hardcoded**.

### Environment Variables

API keys can also be set via environment variables:

```bash
export OPENAI_API_KEY=sk-xxx
export ANTHROPIC_API_KEY=sk-ant-xxx
export DEEPSEEK_API_KEY=xxx
export GROK_API_KEY=xxx
export GROQ_API_KEY=xxx
export TOGETHER_API_KEY=xxx
export MISTRAL_API_KEY=xxx
export OPENROUTER_API_KEY=xxx
```

---

## RAM-Based Model Restrictions

For local models (Ollama), Niko estimates the maximum model size your system can handle:

| System RAM | Max Model Size | 
|------------|---------------|
| 8 GB | ~4B parameters |
| 16 GB | ~12B parameters |
| 32 GB | ~28B parameters |
| 64 GB | ~60B parameters |

Models exceeding your RAM limit are hidden from the selection list. You can still force-select them with a confirmation prompt.

---

## Config File

All settings are stored in `~/.niko/config.yaml`. The file uses a dynamic structure — providers are a map, so you can add as many as you want:

```yaml
active_provider: openai
providers:
  ollama:
    kind: ollama
    base_url: http://127.0.0.1:11434
    model: qwen2.5-coder:7b
  openai:
    kind: openai_compat
    api_key: sk-xxx
    base_url: https://api.openai.com/v1
    model: gpt-4o
  claude:
    kind: anthropic
    api_key: sk-ant-xxx
    model: claude-sonnet-4-20250514
```

---

## Uninstall

```bash
rm $(which niko)
rm -rf ~/.niko
```

## License

MIT
