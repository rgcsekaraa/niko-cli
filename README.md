# Niko

**Natural language to shell command translator.**

```bash
$ niko "find all files larger than 100MB"
find . -type f -size +100M
Copied to clipboard
```

## Features

- **Free & Offline** - Runs locally with Ollama, no API keys required
- **Clipboard** - Command automatically copied, just paste to run
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
Copied to clipboard

$ # Now just paste (Cmd+V / Ctrl+V) and run
```

### Examples

```bash
niko "find python files modified today"
niko "show top 10 processes by memory"
niko "compress logs folder to tar.gz"
niko "search for TODO in all js files"
niko "kill process on port 3000"
niko "git commits from last week"
```

## Configuration

```bash
niko config show
niko config set provider deepseek
niko config set deepseek.api_key sk-xxx
niko config set local.model qwen2.5-coder:7b
```

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
