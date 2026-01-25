# Changelog

## [1.4.0] - 2025-01-26

### Added
- **Interactive prompt** - Shows command with options:
  - `[Tab]` to edit
  - `[Enter]` to run
  - `[Ctrl+C]` to cancel
- **Loading spinner** - Shows "Thinking..." while generating
- **Direct execution** - Use `-x` flag to run immediately

### Changed
- Simplified UX - clean, intuitive command flow
- Cleaner README

## [1.2.0] - 2025-01-25

### Added
- **Auto-select model based on RAM**
  - 8GB+ → qwen2.5-coder:7b
  - 4-8GB → qwen2.5-coder:3b
  - <4GB → qwen2.5-coder:1.5b

### Changed
- Simplified prompt for better accuracy
- Tuned generation parameters

## [1.1.0] - 2025-01-25

### Changed
- Upgraded to qwen2.5-coder:7b by default
- Switched to chat API for better instruction following

### Added
- Welcome screen on first run
- macOS-specific command support

## [1.0.0] - 2025-01-24

### Added
- Natural language to shell command translation
- Local LLM support with Ollama
- Cloud providers: OpenAI, Claude, DeepSeek, Grok
- Safety features and warnings
- Cross-platform support
