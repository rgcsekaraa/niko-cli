package llm

import (
	"context"
	"fmt"

	"github.com/rgcsekaraa/niko-cli/internal/config"
)

type Provider interface {
	Name() string
	Generate(ctx context.Context, systemPrompt, userPrompt string) (string, error)
	IsAvailable() bool
}

type ProviderType string

const (
	ProviderLocal    ProviderType = "local"
	ProviderOpenAI   ProviderType = "openai"
	ProviderClaude   ProviderType = "claude"
	ProviderDeepSeek ProviderType = "deepseek"
	ProviderGrok     ProviderType = "grok"
)

func GetProvider(providerType string) (Provider, error) {
	cfg := config.Get()

	switch ProviderType(providerType) {
	case ProviderLocal:
		return NewLocalProvider(cfg.Local)
	case ProviderOpenAI:
		return NewOpenAIProvider(cfg.OpenAI)
	case ProviderClaude:
		return NewClaudeProvider(cfg.Claude)
	case ProviderDeepSeek:
		return NewDeepSeekProvider(cfg.DeepSeek)
	case ProviderGrok:
		return NewGrokProvider(cfg.Grok)
	default:
		return nil, fmt.Errorf("unknown provider: %s", providerType)
	}
}

func GetDefaultProvider() (Provider, error) {
	cfg := config.Get()
	return GetProvider(cfg.Provider)
}

func ListProviders() []string {
	return []string{
		string(ProviderLocal),
		string(ProviderOpenAI),
		string(ProviderClaude),
		string(ProviderDeepSeek),
		string(ProviderGrok),
	}
}
