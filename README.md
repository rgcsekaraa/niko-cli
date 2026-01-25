# Niko

**Natural language to shell command translator.**

Describe what you want in plain English. Get the exact command.

```bash
$ niko "find all files larger than 100MB"
find . -type f -size +100M

$ niko "kill process on port 3000"
lsof -ti:3000 | xargs kill

$ niko "git commits from last 2 weeks with stats"
git log --since="2 weeks ago" --stat
```

## Features

- **Free & Offline** - Runs locally, no API keys required
- **Interactive Mode** - Run, edit, or cancel commands before execution
- **Smart Model Selection** - Auto-selects the best model based on your RAM
- **Safe** - Warns about dangerous commands, blocks harmful requests
- **Cross-platform** - macOS, Linux, Windows

---

## Install

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/rgcsekaraa/niko-cli/main/scripts/install.sh | sh
```

**Windows (PowerShell):**
```powershell
iwr -useb https://raw.githubusercontent.com/rgcsekaraa/niko-cli/main/scripts/install.ps1 | iex
```

**With Go:**
```bash
go install github.com/niko-cli/niko/cmd/niko@latest
```

---

## First Run

```bash
niko "list files"
```

On first run, Niko detects your system and auto-selects the best model:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Welcome to Niko                              │
└─────────────────────────────────────────────────────────────────┘

Detected: 16GB RAM, 8 CPU cores

Local Models (free, runs on your machine):

  Model                 Size     RAM      Speed     Accuracy
  ─────────────────────────────────────────────────────────────
► qwen2.5-coder:7b      4GB      6GB      Normal    Best
  qwen2.5-coder:3b      2GB      4GB      Fast      Good
  qwen2.5-coder:1.5b    1GB      3GB      Fastest   Basic

Auto-selected: qwen2.5-coder:7b (based on your RAM)
```

**Auto-selection based on RAM:**
- **8GB+ RAM** → 7b model (best accuracy)
- **4-8GB RAM** → 3b model (balanced)
- **<4GB RAM** → 1.5b model (fastest)

The selected model downloads automatically (one-time). After setup, everything runs offline.

---

## Usage

```bash
$ niko "list files by size"

  ⠹ Thinking...

  ls -lhS

  [Enter] Run  [e] Edit  [Esc/q] Cancel
```

**Interactive Controls:**
- **Enter** - Execute the command immediately
- **e** or **Tab** - Edit the command before running
- **Esc** or **q** - Cancel

```bash
# More examples
niko "find python files modified today"
niko "show top 10 processes by memory"
niko "compress logs folder to tar.gz"
niko "search for TODO in all js files"

# Use a cloud provider for a single query
niko -p openai "complex kubernetes deployment"
niko -p claude "optimize this dockerfile"

# Debug mode
niko -v "your query"
```

## Safety

Niko warns about dangerous commands:

```bash
$ niko "recursively delete node_modules"
WARNING: Review before running

find . -type d -name 'node_modules' -prune -exec rm -rf {} +
```

**Risk Levels:**
- **Safe** - Read-only commands (ls, cat, grep) - runs directly
- **Moderate** - State-changing commands (git commit, docker run)
- **Dangerous** - Destructive commands (rm, docker rm) - shows WARNING
- **Critical** - System-destroying commands (rm -rf /) - blocked entirely

---

## Configuration

Config file: `~/.niko/config.yaml`

### View Settings

```bash
niko config show       # Show all settings
niko config get provider
niko config path       # Config file location
```

### Change Model

```bash
# Local models (smaller = faster, larger = more accurate)
niko config set local.model qwen2.5-coder:7b    # Best (default)
niko config set local.model qwen2.5-coder:3b    # Good
niko config set local.model qwen2.5-coder:1.5b  # Basic

# Any Ollama model works
niko config set local.model llama3.2:3b
niko config set local.model deepseek-coder-v2:16b
niko config set local.model codellama:7b
```

### Use Cloud Providers

```bash
# Set provider
niko config set provider deepseek   # Cheap & accurate
niko config set provider openai
niko config set provider claude
niko config set provider grok

# Set API key
niko config set deepseek.api_key sk-xxx
niko config set openai.api_key sk-xxx
niko config set claude.api_key sk-ant-xxx
niko config set grok.api_key xai-xxx

# Or use environment variables
export DEEPSEEK_API_KEY=sk-xxx
export OPENAI_API_KEY=sk-xxx
export ANTHROPIC_API_KEY=sk-ant-xxx
export GROK_API_KEY=xai-xxx
```

### Interactive Mode

```bash
# Disable interactive mode (just print command, don't prompt)
niko config set ui.interactive false

# Enable interactive mode (default)
niko config set ui.interactive true
```

### Advanced

```bash
# Custom Ollama server
niko config set local.url http://192.168.1.100:11434

# OpenAI-compatible APIs (Azure, LocalAI, etc.)
niko config set openai.base_url http://localhost:1234/v1

# Adjust temperature (lower = more deterministic)
niko config set local.temperature 0.1
```

---

## Model Comparison

### Local Models

| Model | Size | RAM | Speed | Accuracy | Best For |
|-------|------|-----|-------|----------|----------|
| qwen2.5-coder:7b | 4GB | 6GB | Normal | Best | Default, most users |
| qwen2.5-coder:3b | 2GB | 4GB | Fast | Good | Limited RAM |
| qwen2.5-coder:1.5b | 1GB | 3GB | Fastest | Basic | Very limited resources |
| deepseek-coder-v2:16b | 9GB | 12GB | Slow | Excellent | Maximum accuracy |

### Cloud Providers

| Provider | Cost | Speed | Accuracy |
|----------|------|-------|----------|
| DeepSeek | ~$0.14/1M tokens | Fast | Excellent |
| OpenAI (gpt-4o-mini) | ~$0.15/1M tokens | Fast | Excellent |
| Claude (haiku) | ~$0.25/1M tokens | Fast | Excellent |
| Grok | ~$5/1M tokens | Fast | Good |

---

## Troubleshooting

### Ollama not starting
```bash
# Check status
curl http://localhost:11434/api/tags

# Start manually
ollama serve
```

### Model not found
```bash
# Pull manually
ollama pull qwen2.5-coder:7b
```

### Inaccurate commands
```bash
# Use a larger model
niko config set local.model deepseek-coder-v2:16b

# Or use cloud provider
niko config set provider deepseek
```

### Slow responses
```bash
# Use a smaller model
niko config set local.model qwen2.5-coder:3b
```

---

## Uninstall

```bash
# Remove binary
rm $(which niko)

# Remove config
rm -rf ~/.niko

# Remove Ollama (optional)
# macOS: rm -rf ~/.ollama && rm /usr/local/bin/ollama
# Linux: rm -rf ~/.ollama && sudo rm /usr/local/bin/ollama
```

---

## Contributing

Contributions welcome! Please submit a Pull Request.

## License

MIT License - see [LICENSE](LICENSE)

## Acknowledgments

- [Ollama](https://ollama.com) - Local LLM runtime
- [Qwen](https://github.com/QwenLM/Qwen2.5-Coder) - Code-optimized models
- [Cobra](https://github.com/spf13/cobra) - CLI framework
