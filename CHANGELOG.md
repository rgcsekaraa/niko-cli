# Changelog

## [2.0.0] - 2026-02-19

### Major Rewrite
- **Rust Rewrite**: Complete rewrite from Go to Rust for improved performance, safety, and maintainability.
- **Streaming Architecture**: All providers now support real-time token streaming for immediate feedback.

### Added
- **Tool Discovery**: Automatically runs `--help` on unknown tools mentioned in queries to learn their flags and syntax dynamically.
- **Smart Context**: `explain` mode uses chunking with 10-line overlap and lossless synthesis for large files.
- **Reliability**: Retry mechanism with exponential backoff (3 attempts), connection pooling, and structured error handling.
- **Ollama Optimizations**: Enabled `flash_attn`, `keep_alive` (30m), and adaptive context windows (4K-16K).
- **Prompt Engineering**: Production-grade system prompts with OS-specific awareness, 50+ examples, and "deep review" analysis.
- **Cross-Platform**: Better detection for macOS, Linux, and Windows (PowerShell) environments.

### Changed
- **Performance**: `cmd` mode is now optimized for speed (512 max tokens), while `explain` uses full context (4096 tokens).
- **Providers**: Unified `Provider` trait supporting Ollama (NDJSON), OpenAI-compatible (SSE), and Claude (SSE).
- **UI**: New TUI with spinners, colored output, and interactive code previews.

### Removed
- **Legacy Go Code**: Removed all Go source files and dependencies.

## [1.5.0] - 2025-01-26

### Changed
- **Simplified UX** - Command prints and copies to clipboard automatically
- Just paste (Cmd+V / Ctrl+V) to run
- Removed interactive prompt complexity

## [1.2.0] - 2025-01-25

### Added
- Auto-select model based on RAM
- Loading spinner while generating

## [1.1.0] - 2025-01-25

### Changed
- Upgraded to qwen2.5-coder:7b by default
- Switched to chat API

## [1.0.0] - 2025-01-24

### Added
- Natural language to shell command translation
- Local LLM support with Ollama
- Cloud providers: OpenAI, Claude, DeepSeek, Grok
- Safety warnings
- Cross-platform support
