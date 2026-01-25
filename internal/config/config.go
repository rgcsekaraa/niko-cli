package config

import (
	"os"
	"path/filepath"
	"sync"

	"github.com/spf13/viper"
	"gopkg.in/yaml.v3"
)

var (
	cfg  *Config
	once sync.Once
)

type Config struct {
	Provider string         `yaml:"provider" mapstructure:"provider"`
	Local    LocalConfig    `yaml:"local" mapstructure:"local"`
	OpenAI   OpenAIConfig   `yaml:"openai" mapstructure:"openai"`
	Claude   ClaudeConfig   `yaml:"claude" mapstructure:"claude"`
	DeepSeek DeepSeekConfig `yaml:"deepseek" mapstructure:"deepseek"`
	Grok     GrokConfig     `yaml:"grok" mapstructure:"grok"`
	Safety   SafetyConfig   `yaml:"safety" mapstructure:"safety"`
	UI       UIConfig       `yaml:"ui" mapstructure:"ui"`
}

type LocalConfig struct {
	Model       string  `yaml:"model" mapstructure:"model"`
	URL         string  `yaml:"url" mapstructure:"url"`
	Threads     int     `yaml:"threads" mapstructure:"threads"`
	ContextSize int     `yaml:"context_size" mapstructure:"context_size"`
	Temperature float64 `yaml:"temperature" mapstructure:"temperature"`
}

type OpenAIConfig struct {
	APIKey      string  `yaml:"api_key" mapstructure:"api_key"`
	Model       string  `yaml:"model" mapstructure:"model"`
	BaseURL     string  `yaml:"base_url" mapstructure:"base_url"`
	Temperature float64 `yaml:"temperature" mapstructure:"temperature"`
}

type ClaudeConfig struct {
	APIKey      string  `yaml:"api_key" mapstructure:"api_key"`
	Model       string  `yaml:"model" mapstructure:"model"`
	Temperature float64 `yaml:"temperature" mapstructure:"temperature"`
}

type DeepSeekConfig struct {
	APIKey      string  `yaml:"api_key" mapstructure:"api_key"`
	Model       string  `yaml:"model" mapstructure:"model"`
	BaseURL     string  `yaml:"base_url" mapstructure:"base_url"`
	Temperature float64 `yaml:"temperature" mapstructure:"temperature"`
}

type GrokConfig struct {
	APIKey      string  `yaml:"api_key" mapstructure:"api_key"`
	Model       string  `yaml:"model" mapstructure:"model"`
	Temperature float64 `yaml:"temperature" mapstructure:"temperature"`
}

type SafetyConfig struct {
	AutoExecute             string   `yaml:"auto_execute" mapstructure:"auto_execute"`
	RequireConfirmDangerous bool     `yaml:"require_confirm_dangerous" mapstructure:"require_confirm_dangerous"`
	BlockedCommands         []string `yaml:"blocked_commands" mapstructure:"blocked_commands"`
}

type UIConfig struct {
	Color   bool `yaml:"color" mapstructure:"color"`
	Verbose bool `yaml:"verbose" mapstructure:"verbose"`
}

func GetConfigDir() string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".niko")
}

func GetConfigPath() string {
	return filepath.Join(GetConfigDir(), "config.yaml")
}

func GetModelsDir() string {
	return filepath.Join(GetConfigDir(), "models")
}

func Load() (*Config, error) {
	var loadErr error
	once.Do(func() {
		cfg = DefaultConfig()

		configDir := GetConfigDir()
		configPath := GetConfigPath()

		if err := os.MkdirAll(configDir, 0755); err != nil {
			loadErr = err
			return
		}

		if _, err := os.Stat(configPath); os.IsNotExist(err) {
			if err := Save(cfg); err != nil {
				loadErr = err
				return
			}
		}

		viper.SetConfigFile(configPath)
		viper.SetConfigType("yaml")

		viper.SetEnvPrefix("NIKO")
		viper.AutomaticEnv()

		if err := viper.ReadInConfig(); err != nil {
			loadErr = err
			return
		}

		if err := viper.Unmarshal(cfg); err != nil {
			loadErr = err
			return
		}

		if envKey := os.Getenv("OPENAI_API_KEY"); envKey != "" && cfg.OpenAI.APIKey == "" {
			cfg.OpenAI.APIKey = envKey
		}
		if envKey := os.Getenv("ANTHROPIC_API_KEY"); envKey != "" && cfg.Claude.APIKey == "" {
			cfg.Claude.APIKey = envKey
		}
		if envKey := os.Getenv("DEEPSEEK_API_KEY"); envKey != "" && cfg.DeepSeek.APIKey == "" {
			cfg.DeepSeek.APIKey = envKey
		}
		if envKey := os.Getenv("GROK_API_KEY"); envKey != "" && cfg.Grok.APIKey == "" {
			cfg.Grok.APIKey = envKey
		}
	})

	return cfg, loadErr
}

func Save(c *Config) error {
	configPath := GetConfigPath()

	if err := os.MkdirAll(filepath.Dir(configPath), 0755); err != nil {
		return err
	}

	data, err := yaml.Marshal(c)
	if err != nil {
		return err
	}

	return os.WriteFile(configPath, data, 0644)
}

func Get() *Config {
	if cfg == nil {
		Load()
	}
	return cfg
}

func Set(key string, value interface{}) error {
	// Ensure config is loaded first
	if _, err := Load(); err != nil {
		return err
	}

	viper.Set(key, value)
	if err := viper.WriteConfig(); err != nil {
		return err
	}
	return viper.Unmarshal(cfg)
}
