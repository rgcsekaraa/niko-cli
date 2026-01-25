# Niko

**Natural language to shell command translator.**

```bash
$ niko "find all files larger than 100MB"

find . -type f -size +100M

[Tab] edit   [Enter] run   [Ctrl+C] cancel
```

## Features

- **Free & Offline** - Runs locally with Ollama, no API keys required
- **Interactive** - Edit, run, or cancel commands before execution
- **Smart** - Auto-selects the best model based on your RAM
- **Safe** - Warns about dangerous commands
- **Cross-platform** - macOS, Linux, Windows

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

## Usage

```bash
$ niko "list files by size"

ls -lhS

[Tab] edit   [Enter] run   [Ctrl+C] cancel
```

- **Enter** - Run the command
- **Tab** - Edit the command first
- **Ctrl+C** - Cancel

### Examples

```bash
niko "find python files modified today"
niko "show top 10 processes by memory"
niko "compress logs folder to tar.gz"
niko "search for TODO in all js files"
niko "kill process on port 3000"
niko "git commits from last week"
```

### Direct Execution

Skip the prompt with `-x`:
```bash
niko -x "list files"
```

### Use Cloud Providers

```bash
niko -p openai "complex kubernetes deployment"
niko -p claude "optimize this dockerfile"
niko -p deepseek "database migration script"
```

## Configuration

```bash
niko config show          # Show all settings
niko config set provider deepseek
niko config set deepseek.api_key sk-xxx
niko config set local.model qwen2.5-coder:7b
```

### Providers

| Provider | Setup |
|----------|-------|
| Local (default) | Works out of the box |
| DeepSeek | `niko config set provider deepseek` |
| OpenAI | `niko config set provider openai` |
| Claude | `niko config set provider claude` |
| Grok | `niko config set provider grok` |

### Local Models

Auto-selected based on RAM:
- **8GB+ RAM** → qwen2.5-coder:7b (best)
- **4-8GB RAM** → qwen2.5-coder:3b (balanced)
- **<4GB RAM** → qwen2.5-coder:1.5b (fastest)

## Uninstall

```bash
rm $(which niko)
rm -rf ~/.niko
```

## License

MIT
