# Changelog

All notable changes to Niko will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.4.0] - 2025-01-26

### Added

- **Shell Integration** - Command appears at your actual shell prompt ready to edit/run
  - Run `niko init` to auto-setup (adds `n` function to your shell)
  - Use `n "query"` and command appears at prompt with cursor
  - Edit naturally with arrow keys, or press Enter to run
- **Direct Execution** - New `-x` flag to execute command immediately
- **Loading Spinner** - Shows animated "Thinking..." while generating commands

### Changed

- Simplified output - just prints the command cleanly
- Removed complex interactive mode in favor of shell integration

## [1.3.0] - 2025-01-25

### Added

- **Interactive Mode** - Commands show with a subtle `⏎` prompt:
  - Press **Enter** to run
  - Press **Tab** or **e** to edit (with full arrow key/backspace support)
  - Press **Esc** or **q** to cancel
- Configurable via `niko config set ui.interactive true/false`

## [1.2.0] - 2025-01-25

### Added

- **Auto-select model based on RAM** - Niko now detects your system RAM and automatically selects the best model:
  - 8GB+ RAM → qwen2.5-coder:7b (best accuracy)
  - 4-8GB RAM → qwen2.5-coder:3b (balanced)
  - <4GB RAM → qwen2.5-coder:1.5b (fastest)
- Shows detected RAM and CPU cores on first run

### Changed

- Simplified system prompt for better accuracy with smaller models
- Tuned generation parameters (temperature=0, tighter sampling) for more deterministic outputs
- Removed overly aggressive safety filtering that caused false positives

### Fixed

- Fixed "Declined: harmful request" false positives on legitimate queries like "run ollama"

## [1.1.0] - 2025-01-25

### Changed

- **Default model upgraded to qwen2.5-coder:7b** for significantly better accuracy
- Switched from Ollama generate API to chat API for improved instruction following
- Improved system prompt with concrete examples for better command generation
- Better response cleaning to extract clean single-line commands

### Added

- **Welcome screen on first run** showing available models and cloud providers
- Auto-detection and use of existing models
- Proper handling of declined/harmful request messages
- Support for macOS-specific commands (e.g., `sed -i ''` instead of `sed -i`)

### Fixed

- Fixed `GetFirstTool` incorrectly parsing piped commands like `env | grep`
- Fixed response cleaning that was outputting garbage text
- Fixed safety prompt to reliably decline harmful requests (crash, format, delete system)
- Fixed model availability check to match exact versions

### Improved

- More accurate commands for complex queries:
  - `git log --since="2 weeks ago" --stat` instead of `git log --oneline -20`
  - `lsof -ti:3000 | xargs kill` instead of incorrect grep-based approach
  - `grep -r "TODO" --include="*.js" .` with proper file filtering
- Better OS-aware command generation (BSD vs GNU flags)

## [1.0.0] - 2025-01-24

### Added

- **Natural language to shell command translation**
  - Describe what you want in plain English
  - Get accurate, executable shell commands
  - OS-aware command generation (macOS, Linux, Windows)

- **Local LLM support (default)**
  - Works offline with Ollama
  - Auto-installs Ollama on first run
  - Support for any Ollama model

- **Cloud provider support**
  - OpenAI (gpt-4o, gpt-4o-mini, gpt-4-turbo)
  - Claude/Anthropic (claude-3-5-sonnet, claude-3-5-haiku)
  - DeepSeek (deepseek-chat, deepseek-coder)
  - Grok (grok-2-latest)

- **Safety features**
  - Dangerous command detection
  - Color-coded warnings (yellow for dangerous, red for critical)
  - Risk level assessment (safe, moderate, dangerous, critical)
  - Blocked commands list

- **Flexible configuration**
  - YAML config file (~/.niko/config.yaml)
  - Environment variable support
  - Per-provider model selection
  - Temperature control
  - Custom API endpoints (OpenAI-compatible)

- **Cross-platform support**
  - macOS (Intel and Apple Silicon)
  - Linux (amd64 and arm64)
  - Windows (amd64 and arm64)

- **Easy installation**
  - One-line curl/PowerShell install
  - Go install support
  - Pre-built binaries

---

## Roadmap

### Planned Features

- [x] ~~Interactive mode~~ (added in v1.3.0)
- [x] ~~Shell integration (execute commands directly)~~ (added in v1.3.0)
- [ ] Command history and suggestions
- [ ] Command explanation mode (`niko explain "complex command"`)
- [ ] Pipe input support

### Under Consideration

- Homebrew formula
- AUR package
- Windows Scoop manifest
