package llm

import (
	"context"
	"strings"

	"github.com/anthropics/anthropic-sdk-go"
	"github.com/anthropics/anthropic-sdk-go/option"
	"github.com/rgcsekaraa/niko-cli/internal/config"
)

type ClaudeProvider struct {
	client *anthropic.Client
	model  string
	apiKey string
}

func NewClaudeProvider(cfg config.ClaudeConfig) (*ClaudeProvider, error) {
	client := anthropic.NewClient(option.WithAPIKey(cfg.APIKey))

	return &ClaudeProvider{
		client: client,
		model:  cfg.Model,
		apiKey: cfg.APIKey,
	}, nil
}

func (p *ClaudeProvider) Name() string {
	return "claude"
}

func (p *ClaudeProvider) IsAvailable() bool {
	return p.apiKey != ""
}

func (p *ClaudeProvider) Generate(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	message, err := p.client.Messages.New(ctx, anthropic.MessageNewParams{
		Model:     anthropic.F(p.model),
		MaxTokens: anthropic.F(int64(500)),
		System: anthropic.F([]anthropic.TextBlockParam{
			anthropic.NewTextBlock(systemPrompt),
		}),
		Messages: anthropic.F([]anthropic.MessageParam{
			anthropic.NewUserMessage(anthropic.NewTextBlock(userPrompt)),
		}),
	})

	if err != nil {
		return "", err
	}

	if len(message.Content) == 0 {
		return "", nil
	}

	var result strings.Builder
	for _, block := range message.Content {
		if block.Type == anthropic.ContentBlockTypeText {
			result.WriteString(block.Text)
		}
	}

	return strings.TrimSpace(result.String()), nil
}
