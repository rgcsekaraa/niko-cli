package config

import "runtime"

func DefaultConfig() *Config {
	threads := runtime.NumCPU()
	if threads > 4 {
		threads = 4
	}

	return &Config{
		Provider: "local", // Local Ollama by default - free and fast
		Local: LocalConfig{
			Model:       "qwen2.5-coder:7b", // Best accuracy for shell commands
			URL:         "http://127.0.0.1:11434",
			Threads:     threads,
			ContextSize: 4096,
			Temperature: 0.1, // Low temp for deterministic command generation
		},
		OpenAI: OpenAIConfig{
			APIKey:      "",
			Model:       "gpt-4o-mini",
			BaseURL:     "", // Empty uses default OpenAI API
			Temperature: 0.1,
		},
		Claude: ClaudeConfig{
			APIKey:      "",
			Model:       "claude-3-5-haiku-20241022",
			Temperature: 0.1,
		},
		DeepSeek: DeepSeekConfig{
			APIKey:      "",
			Model:       "deepseek-chat",
			BaseURL:     "https://api.deepseek.com",
			Temperature: 0.1,
		},
		Grok: GrokConfig{
			APIKey:      "",
			Model:       "grok-2-latest",
			Temperature: 0.1,
		},
		Safety: SafetyConfig{
			AutoExecute:             "safe",
			RequireConfirmDangerous: true,
			BlockedCommands: []string{
				"rm -rf /",
				"rm -rf /*",
				":(){ :|:& };:",
				"dd if=/dev/zero of=/dev/sda",
				"mkfs.ext4 /dev/sda",
				"> /dev/sda",
				"chmod -R 777 /",
			},
		},
		UI: UIConfig{
			Color:   true,
			Verbose: false,
		},
	}
}
