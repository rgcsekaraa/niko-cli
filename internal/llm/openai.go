package llm

import (
	"context"
	"strings"

	"github.com/niko-cli/niko/internal/config"
	"github.com/sashabaranov/go-openai"
)

type OpenAIProvider struct {
	client *openai.Client
	model  string
	apiKey string
}

func NewOpenAIProvider(cfg config.OpenAIConfig) (*OpenAIProvider, error) {
	clientConfig := openai.DefaultConfig(cfg.APIKey)
	if cfg.BaseURL != "" {
		clientConfig.BaseURL = cfg.BaseURL
	}

	return &OpenAIProvider{
		client: openai.NewClientWithConfig(clientConfig),
		model:  cfg.Model,
		apiKey: cfg.APIKey,
	}, nil
}

func (p *OpenAIProvider) Name() string {
	return "openai"
}

func (p *OpenAIProvider) IsAvailable() bool {
	return p.apiKey != ""
}

func (p *OpenAIProvider) Generate(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	resp, err := p.client.CreateChatCompletion(ctx, openai.ChatCompletionRequest{
		Model: p.model,
		Messages: []openai.ChatCompletionMessage{
			{
				Role:    openai.ChatMessageRoleSystem,
				Content: systemPrompt,
			},
			{
				Role:    openai.ChatMessageRoleUser,
				Content: userPrompt,
			},
		},
		Temperature: 0.1,
		MaxTokens:   500,
	})

	if err != nil {
		return "", err
	}

	if len(resp.Choices) == 0 {
		return "", nil
	}

	return strings.TrimSpace(resp.Choices[0].Message.Content), nil
}
