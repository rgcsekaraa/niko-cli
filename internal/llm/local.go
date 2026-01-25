package llm

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"runtime"
	"strings"
	"time"

	"github.com/niko-cli/niko/internal/config"
	"github.com/pbnjay/memory"
)

const (
	DefaultModel     = "qwen2.5-coder:1.5b" // Base model, fastest
	FallbackModel    = "qwen2.5-coder:1.5b" // Fallback
	OllamaDefaultURL = "http://127.0.0.1:11434"
)

// ModelOption represents a model choice for interactive selection
type ModelOption struct {
	Number   string
	Model    string
	Size     string
	RAM      string
	Speed    string
	Accuracy string
}

var availableModels = []ModelOption{
	{"1", "qwen2.5-coder:7b", "4GB", "6GB", "Normal", "Best"},
	{"2", "qwen2.5-coder:3b", "2GB", "4GB", "Fast", "Good"},
	{"3", "qwen2.5-coder:1.5b", "1GB", "3GB", "Fastest", "Basic"},
}

type LocalProvider struct {
	baseURL     string
	model       string
	temperature float64
	client      *http.Client
	manager     *OllamaManager
}

func NewLocalProvider(cfg config.LocalConfig) (*LocalProvider, error) {
	baseURL := cfg.URL
	if baseURL == "" {
		baseURL = OllamaDefaultURL
	}

	model := cfg.Model
	if model == "" || model == "tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf" {
		model = DefaultModel
	}

	temp := cfg.Temperature
	if temp <= 0 {
		temp = 0.1
	}

	return &LocalProvider{
		baseURL:     baseURL,
		model:       model,
		temperature: temp,
		client: &http.Client{
			Timeout: 120 * time.Second,
		},
		manager: NewOllamaManager(),
	}, nil
}

func (p *LocalProvider) Name() string {
	return "local"
}

func (p *LocalProvider) IsAvailable() bool {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	req, _ := http.NewRequestWithContext(ctx, "GET", p.baseURL+"/api/tags", nil)
	resp, err := p.client.Do(req)
	if err != nil {
		return false
	}
	defer resp.Body.Close()
	return resp.StatusCode == http.StatusOK
}

func (p *LocalProvider) Generate(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	if !p.IsAvailable() {
		if err := p.EnsureRunning(ctx); err != nil {
			return "", fmt.Errorf("ollama not available: %w\n\nInstall Ollama: https://ollama.com/download", err)
		}
	}

	if err := p.EnsureModelExists(ctx); err != nil {
		return "", err
	}

	// Use chat API for better instruction following
	messages := []map[string]string{
		{"role": "system", "content": systemPrompt},
		{"role": "user", "content": userPrompt},
	}

	reqBody := map[string]interface{}{
		"model":    p.model,
		"messages": messages,
		"stream":   false,
		"options": map[string]interface{}{
			"temperature":    0.0,  // Deterministic for accurate commands
			"num_predict":    100,  // Commands are short
			"top_p":          0.7,  // More focused sampling
			"top_k":          20,   // Limit vocabulary for precision
			"repeat_penalty": 1.2,  // Avoid repetition
			"stop":           []string{"\n", "```", "Explanation:", "Note:", "#", "//"},
		},
	}

	jsonBody, _ := json.Marshal(reqBody)
	req, err := http.NewRequestWithContext(ctx, "POST", p.baseURL+"/api/chat", bytes.NewReader(jsonBody))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := p.client.Do(req)
	if err != nil {
		return "", fmt.Errorf("failed to call ollama: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("ollama error: %s", string(body))
	}

	var result struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return "", err
	}

	return cleanResponse(result.Message.Content), nil
}

func cleanResponse(response string) string {
	response = strings.TrimSpace(response)

	// Remove markdown code blocks if present
	if strings.Contains(response, "```") {
		lines := strings.Split(response, "\n")
		var cleaned []string
		inBlock := false
		for _, line := range lines {
			trimmed := strings.TrimSpace(line)
			if strings.HasPrefix(trimmed, "```") {
				inBlock = !inBlock
				continue
			}
			if inBlock {
				cleaned = append(cleaned, line)
			}
		}
		if len(cleaned) > 0 {
			response = strings.Join(cleaned, "\n")
		}
	}

	// Remove common prefixes
	prefixes := []string{"$ ", "> ", "Command: ", "command: ", "Output: "}
	for _, prefix := range prefixes {
		response = strings.TrimPrefix(response, prefix)
	}

	// Take the first valid command line
	lines := strings.Split(response, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		// Skip empty lines, comments, and explanatory text
		if line == "" ||
			strings.HasPrefix(line, "#") ||
			strings.HasPrefix(line, "//") ||
			strings.HasPrefix(line, "Note:") ||
			strings.HasPrefix(line, "This ") ||
			strings.HasPrefix(line, "The ") ||
			strings.HasPrefix(line, "'") && strings.Contains(line, "not installed") {
			continue
		}
		return line
	}

	return strings.TrimSpace(response)
}

func (p *LocalProvider) EnsureRunning(ctx context.Context) error {
	if p.IsAvailable() {
		return nil
	}

	fmt.Println("Setting up local AI (one-time setup)...")
	fmt.Println()

	// Auto-install Ollama if not present
	if err := p.manager.EnsureInstalled(func(status string, pct float64) {
		if pct > 0 {
			fmt.Printf("\r  %s %.0f%%", status, pct)
		} else {
			fmt.Printf("\r  %s", status)
		}
	}); err != nil {
		return fmt.Errorf("failed to setup Ollama: %w", err)
	}
	fmt.Println()

	fmt.Print("  Starting AI server...")
	_, err := p.manager.StartServer()
	if err != nil {
		return fmt.Errorf("failed to start Ollama: %w", err)
	}
	fmt.Println(" ready")

	return nil
}

func (p *LocalProvider) EnsureModelExists(ctx context.Context) error {
	if p.hasModel(ctx, p.model) {
		return nil
	}

	// Check if user has any of our recommended models already
	for _, opt := range availableModels {
		if p.hasModel(ctx, opt.Model) {
			p.model = opt.Model
			return nil
		}
	}

	// First time setup - let user choose a model
	selectedModel, err := p.selectModel()
	if err != nil {
		return err
	}
	p.model = selectedModel

	// Save the selection to config
	if err := config.Set("local.model", selectedModel); err != nil {
		// Non-fatal, continue anyway
		fmt.Fprintf(os.Stderr, "Warning: could not save model preference: %v\n", err)
	}

	fmt.Println()
	fmt.Printf("Downloading '%s'...\n", p.model)

	if err := p.pullModel(ctx, p.model); err != nil {
		if p.model != FallbackModel {
			fmt.Printf("Trying fallback model '%s'...\n", FallbackModel)
			p.model = FallbackModel
			if p.hasModel(ctx, FallbackModel) {
				return nil
			}
			return p.pullModel(ctx, FallbackModel)
		}
		return err
	}

	return nil
}

// selectModelByRAM picks the best model based on available system RAM
func selectModelByRAM() string {
	totalRAM := memory.TotalMemory()
	ramGB := totalRAM / (1024 * 1024 * 1024)

	// Model RAM requirements:
	// 7b: needs 6GB+ RAM
	// 3b: needs 4GB+ RAM
	// 1.5b: needs 3GB+ RAM (fallback)
	if ramGB >= 8 {
		return "qwen2.5-coder:7b"
	} else if ramGB >= 4 {
		return "qwen2.5-coder:3b"
	}
	return "qwen2.5-coder:1.5b"
}

func (p *LocalProvider) selectModel() (string, error) {
	selectedModel := selectModelByRAM()
	totalRAM := memory.TotalMemory()
	ramGB := totalRAM / (1024 * 1024 * 1024)

	fmt.Println()
	fmt.Println("┌─────────────────────────────────────────────────────────────────┐")
	fmt.Println("│                    Welcome to Niko                              │")
	fmt.Println("└─────────────────────────────────────────────────────────────────┘")
	fmt.Println()
	fmt.Printf("Detected: %dGB RAM, %d CPU cores\n", ramGB, runtime.NumCPU())
	fmt.Println()
	fmt.Println("Local Models (free, runs on your machine):")
	fmt.Println()
	fmt.Println("  Model                 Size     RAM      Speed     Accuracy")
	fmt.Println("  ─────────────────────────────────────────────────────────────")
	for _, opt := range availableModels {
		marker := "  "
		if opt.Model == selectedModel {
			marker = "► "
		}
		fmt.Printf("  %s%-19s %-8s %-8s %-9s %s\n",
			marker, opt.Model, opt.Size, opt.RAM, opt.Speed, opt.Accuracy)
	}
	fmt.Println()
	fmt.Println("Cloud Providers (need API key, more accurate):")
	fmt.Println()
	fmt.Println("  Provider     Model                  Setup")
	fmt.Println("  ─────────────────────────────────────────────────────────────")
	fmt.Println("  DeepSeek     deepseek-chat          niko config set provider deepseek")
	fmt.Println("  OpenAI       gpt-4o-mini            niko config set provider openai")
	fmt.Println("  Claude       claude-3-5-haiku       niko config set provider claude")
	fmt.Println("  Grok         grok-2-latest          niko config set provider grok")
	fmt.Println()
	fmt.Println("─────────────────────────────────────────────────────────────────")
	fmt.Println()
	fmt.Printf("Auto-selected: %s (based on your RAM)\n", selectedModel)
	fmt.Println("Change later: niko config set local.model <model>")
	fmt.Println()

	return selectedModel, nil
}

func (p *LocalProvider) hasModel(ctx context.Context, model string) bool {
	req, _ := http.NewRequestWithContext(ctx, "GET", p.baseURL+"/api/tags", nil)
	resp, err := p.client.Do(req)
	if err != nil {
		return false
	}
	defer resp.Body.Close()

	var result struct {
		Models []struct {
			Name string `json:"name"`
		} `json:"models"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return false
	}

	for _, m := range result.Models {
		// Exact match: qwen2.5-coder:7b == qwen2.5-coder:7b
		if m.Name == model {
			return true
		}
		// Match with tag variations: qwen2.5-coder:7b matches qwen2.5-coder:7b-q4_0
		if strings.Contains(model, ":") && strings.HasPrefix(m.Name, model) {
			return true
		}
	}
	return false
}

func (p *LocalProvider) pullModel(ctx context.Context, model string) error {
	reqBody := map[string]interface{}{
		"name":   model,
		"stream": true,
	}
	jsonBody, _ := json.Marshal(reqBody)

	req, err := http.NewRequestWithContext(ctx, "POST", p.baseURL+"/api/pull", bytes.NewReader(jsonBody))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := p.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	scanner := bufio.NewScanner(resp.Body)
	var lastStatus string
	for scanner.Scan() {
		var progress struct {
			Status    string `json:"status"`
			Completed int64  `json:"completed"`
			Total     int64  `json:"total"`
		}
		if err := json.Unmarshal(scanner.Bytes(), &progress); err != nil {
			continue
		}

		if progress.Status != lastStatus {
			if progress.Total > 0 {
				pct := float64(progress.Completed) / float64(progress.Total) * 100
				fmt.Printf("\r  %s: %.1f%%", progress.Status, pct)
			} else {
				fmt.Printf("\r  %s", progress.Status)
			}
			lastStatus = progress.Status
		} else if progress.Total > 0 {
			pct := float64(progress.Completed) / float64(progress.Total) * 100
			fmt.Printf("\r  %s: %.1f%%", progress.Status, pct)
		}
	}
	fmt.Println()

	return nil
}

func (p *LocalProvider) GetModel() string {
	return p.model
}

func IsOllamaRunning() bool {
	resp, err := http.Get(OllamaDefaultURL + "/api/tags")
	if err != nil {
		return false
	}
	defer resp.Body.Close()
	return resp.StatusCode == http.StatusOK
}

func (p *LocalProvider) GetManager() *OllamaManager {
	return p.manager
}
