package cli

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/fatih/color"
	"github.com/rgcsekaraa/niko-cli/internal/config"
	"github.com/rgcsekaraa/niko-cli/internal/executor"
	"github.com/rgcsekaraa/niko-cli/internal/llm"
	"github.com/rgcsekaraa/niko-cli/internal/prompt"
	"github.com/spf13/cobra"
)

var (
	yellow = color.New(color.FgYellow, color.Bold)
	red    = color.New(color.FgRed, color.Bold)
)

var (
	Version   = "dev"
	CommitSHA = "unknown"
	BuildDate = "unknown"

	providerFlag string
	verboseFlag  bool
)

func NewRootCmd() *cobra.Command {
	rootCmd := &cobra.Command{
		Use:   "niko <query>",
		Short: "AI-powered natural language to shell command translator",
		Long: `Niko translates natural language queries into executable shell commands.

Usage:
  niko "find all large files"
  niko "show docker containers with nginx"
  niko "git commits from last week by john"

The generated command is printed to stdout for you to review and execute.`,
		Example: `  niko "list files modified today"
  niko "compress all jpg files in current folder"
  niko "show processes using port 8080"`,
		Args:                  cobra.MinimumNArgs(1),
		DisableFlagsInUseLine: true,
		SilenceUsage:          true,
		SilenceErrors:         true,
		RunE:                  runQuery,
	}

	rootCmd.Flags().StringVarP(&providerFlag, "provider", "p", "", "override default provider (local|openai|claude|deepseek|grok)")
	rootCmd.Flags().BoolVarP(&verboseFlag, "verbose", "v", false, "show debug information")

	rootCmd.AddCommand(newConfigCmd())
	rootCmd.AddCommand(newVersionCmd())

	return rootCmd
}

func runQuery(cmd *cobra.Command, args []string) error {
	query := strings.Join(args, " ")

	// Handle self-referential queries about niko itself
	lowerQuery := strings.ToLower(query)
	if strings.Contains(lowerQuery, "niko") || strings.Contains(lowerQuery, "how to run") && strings.Contains(lowerQuery, "this") {
		return handleNikoQuery(lowerQuery)
	}

	return processQuery(query)
}

func handleNikoQuery(query string) error {
	if strings.Contains(query, "help") || strings.Contains(query, "how to use") || strings.Contains(query, "how to run") {
		fmt.Println("niko --help")
		return nil
	}
	if strings.Contains(query, "version") {
		fmt.Println("niko version")
		return nil
	}
	if strings.Contains(query, "config") || strings.Contains(query, "setup") {
		fmt.Println("niko config show")
		return nil
	}
	// Default help
	fmt.Println("niko --help")
	return nil
}

func processQuery(query string) error {
	cfg, err := config.Load()
	if err != nil {
		red.Println("Failed to load config")
		fmt.Printf("Try: niko config init\n")
		return nil
	}

	providerName := cfg.Provider
	if providerFlag != "" {
		providerName = providerFlag
	}

	provider, err := llm.GetProvider(providerName)
	if err != nil {
		red.Printf("Unknown provider: %s\n", providerName)
		fmt.Println("Available: local, openai, claude, deepseek, grok")
		return nil
	}

	if providerName != "local" && !provider.IsAvailable() {
		yellow.Printf("Provider '%s' not configured\n\n", providerName)
		fmt.Println("Set up with:")
		fmt.Printf("  niko config set %s.api_key <your-key>\n\n", providerName)
		fmt.Println("Or set as environment variable:")
		fmt.Printf("  export %s=<your-key>\n", getEnvVarName(providerName))
		return nil
	}

	if verboseFlag {
		_, _ = fmt.Fprintf(os.Stderr, "[debug] provider: %s\n", provider.Name())
	}

	ctx := context.Background()
	sysCtx := prompt.GatherContext()
	systemPrompt := prompt.BuildSystemPrompt(sysCtx)
	userPrompt := prompt.BuildUserPrompt(query)

	// Show spinner while generating
	spinner := NewSpinner("Thinking...")
	spinner.Start()

	start := time.Now()
	response, err := provider.Generate(ctx, systemPrompt, userPrompt)
	spinner.Stop()

	if err != nil {
		red.Println("Generation failed")
		dim := color.New(color.Faint)
		dim.Printf("(%v)\n", err)
		return nil
	}

	if verboseFlag {
		_, _ = fmt.Fprintf(os.Stderr, "[debug] response time: %v\n", time.Since(start))
	}

	command := executor.ExtractCommand(response)
	if command == "" {
		yellow.Println("Could not generate a command")
		fmt.Println("Try being more specific")
		return nil
	}

	// Handle declined/special messages from the LLM
	if strings.HasPrefix(command, "Declined:") || strings.HasPrefix(command, "Please specify:") {
		yellow.Println(command)
		return nil
	}
	if strings.HasPrefix(command, "echo \"Declined:") || strings.HasPrefix(command, "echo \"Please specify:") {
		// Extract the message from echo command
		msg := strings.TrimPrefix(command, "echo \"")
		msg = strings.TrimSuffix(msg, "\"")
		yellow.Println(msg)
		return nil
	}

	// Check if the tool exists
	tool := executor.GetFirstTool(command)
	if tool != "" && !executor.IsToolAvailable(tool) {
		dim := color.New(color.Faint)
		dim.Printf("('%s' not found - install it first)\n", tool)
	}

	// Print command and copy to clipboard
	fmt.Println(command)
	CopyToClipboard(command)

	// Show subtle warning if dangerous
	risk := executor.AssessRisk(command)
	dim := color.New(color.Faint)
	if risk == executor.Critical {
		dim.Println("(destructive command)")
	} else if risk == executor.Dangerous {
		dim.Println("(review before running)")
	}

	return nil
}

func newConfigCmd() *cobra.Command {
	configCmd := &cobra.Command{
		Use:   "config",
		Short: "View and modify configuration",
		Long: `Manage Niko configuration settings.

Configuration file: ~/.niko/config.yaml

Environment variables (override config file):
  OPENAI_API_KEY      OpenAI API key
  ANTHROPIC_API_KEY   Claude API key
  DEEPSEEK_API_KEY    DeepSeek API key
  GROK_API_KEY        Grok API key`,
	}

	// config show
	configCmd.AddCommand(&cobra.Command{
		Use:   "show",
		Short: "Display current configuration",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := config.Load()
			if err != nil {
				return err
			}

			fmt.Printf("Configuration: %s\n\n", config.GetConfigPath())
			fmt.Printf("Provider: %s\n\n", cfg.Provider)
			fmt.Println("API Keys:")
			fmt.Printf("  openai    %s\n", formatKeyStatus(cfg.OpenAI.APIKey, "OPENAI_API_KEY"))
			fmt.Printf("  claude    %s\n", formatKeyStatus(cfg.Claude.APIKey, "ANTHROPIC_API_KEY"))
			fmt.Printf("  deepseek  %s\n", formatKeyStatus(cfg.DeepSeek.APIKey, "DEEPSEEK_API_KEY"))
			fmt.Printf("  grok      %s\n", formatKeyStatus(cfg.Grok.APIKey, "GROK_API_KEY"))
			fmt.Printf("  local     %s\n", localStatus(cfg))

			return nil
		},
	})

	// config get
	configCmd.AddCommand(&cobra.Command{
		Use:   "get <key>",
		Short: "Get a configuration value",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, err := config.Load()
			if err != nil {
				return err
			}

			key := args[0]
			switch key {
			case "provider":
				fmt.Println(cfg.Provider)
			// Local
			case "local.model":
				fmt.Println(cfg.Local.Model)
			case "local.url":
				fmt.Println(cfg.Local.URL)
			case "local.temperature":
				fmt.Println(cfg.Local.Temperature)
			// OpenAI
			case "openai.model":
				fmt.Println(cfg.OpenAI.Model)
			case "openai.base_url":
				fmt.Println(cfg.OpenAI.BaseURL)
			case "openai.temperature":
				fmt.Println(cfg.OpenAI.Temperature)
			// Claude
			case "claude.model":
				fmt.Println(cfg.Claude.Model)
			case "claude.temperature":
				fmt.Println(cfg.Claude.Temperature)
			// DeepSeek
			case "deepseek.model":
				fmt.Println(cfg.DeepSeek.Model)
			case "deepseek.base_url":
				fmt.Println(cfg.DeepSeek.BaseURL)
			case "deepseek.temperature":
				fmt.Println(cfg.DeepSeek.Temperature)
			// Grok
			case "grok.model":
				fmt.Println(cfg.Grok.Model)
			case "grok.temperature":
				fmt.Println(cfg.Grok.Temperature)
			default:
				return fmt.Errorf("unknown key: %s\nRun 'niko config show' to see available options", key)
			}
			return nil
		},
	})

	// config set
	configCmd.AddCommand(&cobra.Command{
		Use:   "set <key> <value>",
		Short: "Set a configuration value",
		Long: `Set a configuration value.

Available keys:
  provider                Default provider (local|openai|claude|deepseek|grok)

  local.model             Local model name (any Ollama model)
  local.url               Ollama API URL (default: http://127.0.0.1:11434)
  local.temperature       Temperature for generation (0.0-1.0)

  openai.api_key          OpenAI API key
  openai.model            OpenAI model (gpt-4o, gpt-4o-mini, gpt-4-turbo, etc.)
  openai.base_url         Custom OpenAI-compatible API URL (for Azure, local, etc.)
  openai.temperature      Temperature for generation (0.0-1.0)

  claude.api_key          Claude/Anthropic API key
  claude.model            Claude model (claude-3-5-sonnet-20241022, etc.)
  claude.temperature      Temperature for generation (0.0-1.0)

  deepseek.api_key        DeepSeek API key
  deepseek.model          DeepSeek model (deepseek-chat, deepseek-coder, etc.)
  deepseek.base_url       DeepSeek API URL
  deepseek.temperature    Temperature for generation (0.0-1.0)

  grok.api_key            Grok API key
  grok.model              Grok model (grok-2-latest, etc.)
  grok.temperature        Temperature for generation (0.0-1.0)`,
		Example: `  niko config set provider openai
  niko config set openai.api_key sk-xxx
  niko config set local.model llama3.2:3b
  niko config set local.model deepseek-r1:1.5b
  niko config set openai.base_url http://localhost:1234/v1
  niko config set local.temperature 0.2`,
		Args: cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			key, value := args[0], args[1]
			if err := config.Set(key, value); err != nil {
				return err
			}
			if strings.Contains(key, "api_key") {
				fmt.Printf("%s: configured\n", key)
			} else {
				fmt.Printf("%s: %s\n", key, value)
			}
			return nil
		},
	})

	// config path
	configCmd.AddCommand(&cobra.Command{
		Use:   "path",
		Short: "Print configuration file path",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Println(config.GetConfigPath())
		},
	})

	// config init
	configCmd.AddCommand(&cobra.Command{
		Use:   "init",
		Short: "Initialize configuration with defaults",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg := config.DefaultConfig()
			if err := config.Save(cfg); err != nil {
				return err
			}
			fmt.Printf("Created: %s\n", config.GetConfigPath())
			return nil
		},
	})

	return configCmd
}

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print version information",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Printf("niko %s\n", Version)
			if verboseFlag {
				fmt.Printf("commit: %s\n", CommitSHA)
				fmt.Printf("built:  %s\n", BuildDate)
			}
		},
	}
}

func formatKeyStatus(key, envVar string) string {
	if key != "" {
		if len(key) > 8 {
			return key[:4] + "..." + key[len(key)-4:]
		}
		return "****"
	}
	if os.Getenv(envVar) != "" {
		return "(from $" + envVar + ")"
	}
	return "-"
}

func localStatus(cfg *config.Config) string {
	manager := llm.NewOllamaManager()
	if !manager.IsInstalled() && !manager.IsSystemOllamaAvailable() {
		return "auto-install on first use"
	}
	if !llm.IsOllamaRunning() {
		return "ready"
	}
	return "running (" + cfg.Local.Model + ")"
}

func getEnvVarName(provider string) string {
	switch provider {
	case "claude":
		return "ANTHROPIC_API_KEY"
	case "openai":
		return "OPENAI_API_KEY"
	case "deepseek":
		return "DEEPSEEK_API_KEY"
	case "grok":
		return "GROK_API_KEY"
	default:
		return strings.ToUpper(provider) + "_API_KEY"
	}
}

