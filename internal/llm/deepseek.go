package llm

import (
	"context"
	"strings"

	"github.com/rgcsekaraa/niko-cli/internal/config"
	"github.com/sashabaranov/go-openai"
)

type DeepSeekProvider struct {
	client *openai.Client
	model  string
	apiKey string
}

func NewDeepSeekProvider(cfg config.DeepSeekConfig) (*DeepSeekProvider, error) {
	clientConfig := openai.DefaultConfig(cfg.APIKey)
	clientConfig.BaseURL = "https://api.deepseek.com/v1"

	return &DeepSeekProvider{
		client: openai.NewClientWithConfig(clientConfig),
		model:  cfg.Model,
		apiKey: cfg.APIKey,
	}, nil
}

func (p *DeepSeekProvider) Name() string {
	return "deepseek"
}

func (p *DeepSeekProvider) IsAvailable() bool {
	return p.apiKey != ""
}

func (p *DeepSeekProvider) Generate(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
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
