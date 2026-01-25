package cli

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/fatih/color"
	"github.com/niko-cli/niko/internal/config"
	"github.com/niko-cli/niko/internal/executor"
	"github.com/niko-cli/niko/internal/llm"
	"github.com/niko-cli/niko/internal/prompt"
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
	execFlag     bool
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
  niko "show processes using port 8080"
  niko --provider openai "deploy to kubernetes"`,
		Args:                  cobra.MinimumNArgs(1),
		DisableFlagsInUseLine: true,
		RunE:                  runQuery,
	}

	rootCmd.Flags().StringVarP(&providerFlag, "provider", "p", "", "override default provider (local|openai|claude|deepseek|grok)")
	rootCmd.Flags().BoolVarP(&verboseFlag, "verbose", "v", false, "show debug information")
	rootCmd.Flags().BoolVarP(&execFlag, "exec", "x", false, "execute the command directly")

	rootCmd.AddCommand(newConfigCmd())
	rootCmd.AddCommand(newVersionCmd())
	rootCmd.AddCommand(newInitCmd())

	return rootCmd
}

func runQuery(cmd *cobra.Command, args []string) error {
	// Auto-setup shell integration on first run
	checkAndSetupShell()

	query := strings.Join(args, " ")

	// Handle self-referential queries about niko itself
	lowerQuery := strings.ToLower(query)
	if strings.Contains(lowerQuery, "niko") || strings.Contains(lowerQuery, "how to run") && strings.Contains(lowerQuery, "this") {
		return handleNikoQuery(lowerQuery)
	}

	return processQuery(query)
}

func checkAndSetupShell() {
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}

	shell := os.Getenv("SHELL")
	var rcFile string

	if strings.Contains(shell, "zsh") {
		rcFile = home + "/.zshrc"
	} else if strings.Contains(shell, "bash") {
		rcFile = home + "/.bashrc"
	} else {
		return
	}

	// Check if already installed
	content, err := os.ReadFile(rcFile)
	if err != nil || strings.Contains(string(content), "Niko") {
		return // Already installed or can't read
	}

	// First run - install and notify
	setupShellIntegration()
	fmt.Fprintln(os.Stderr, "")
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
		return fmt.Errorf("config error: %w", err)
	}

	providerName := cfg.Provider
	if providerFlag != "" {
		providerName = providerFlag
	}

	provider, err := llm.GetProvider(providerName)
	if err != nil {
		return fmt.Errorf("provider error: %w", err)
	}

	if providerName != "local" && !provider.IsAvailable() {
		return fmt.Errorf("provider '%s' not configured. Set API key:\n  niko config set %s.api_key <your-key>\n  or: export %s_API_KEY=<your-key>",
			providerName, providerName, strings.ToUpper(providerName))
	}

	if verboseFlag {
		fmt.Fprintf(os.Stderr, "[debug] provider: %s\n", provider.Name())
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
		return fmt.Errorf("generation failed: %w", err)
	}

	if verboseFlag {
		fmt.Fprintf(os.Stderr, "[debug] response time: %v\n", time.Since(start))
	}

	command := executor.ExtractCommand(response)
	if command == "" {
		return fmt.Errorf("could not generate a valid command")
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
		yellow.Printf("'%s' is not installed\n", tool)
		fmt.Println()
	}

	// Check for dangerous commands
	risk := executor.AssessRisk(command)
	if risk == executor.Critical {
		red.Fprintln(os.Stderr, "DANGER: This command is destructive!")
		fmt.Println(command)
		return nil
	}
	if risk == executor.Dangerous {
		yellow.Fprintln(os.Stderr, "WARNING: Review before running")
	}

	// Execute directly if -x flag is set
	if execFlag {
		fmt.Println(command)
		return ExecuteCommand(command)
	}

	// Default: just print the command
	fmt.Println(command)
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
			// UI
			case "ui.interactive":
				fmt.Println(cfg.UI.Interactive)
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
  grok.temperature        Temperature for generation (0.0-1.0)

  ui.interactive          Enable interactive mode (true|false)`,
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

func newInitCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "init",
		Short: "Setup shell integration",
		Long: `Sets up tab-completion style integration.

After setup, type 'niko' followed by your query and press Tab:
  $ niko list files<Tab>
  $ ls -la█   <-- command appears, ready to edit/run`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return setupShellIntegration()
		},
	}
}

func setupShellIntegration() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}

	shell := os.Getenv("SHELL")
	var rcFile string
	var shellCode string

	if strings.Contains(shell, "zsh") {
		rcFile = home + "/.zshrc"
		shellCode = `
# Niko - press Tab after 'niko your query' to get the command
_niko_complete() {
    if [[ "$LBUFFER" == niko\ * ]]; then
        local query="${LBUFFER#niko }"
        local cmd=$(niko "$query" 2>/dev/null)
        if [ -n "$cmd" ]; then
            LBUFFER="$cmd"
            zle redisplay
        fi
    else
        zle expand-or-complete
    fi
}
zle -N _niko_complete
bindkey '^I' _niko_complete
`
	} else if strings.Contains(shell, "bash") {
		rcFile = home + "/.bashrc"
		shellCode = `
# Niko - press Tab after 'niko your query' to get the command
_niko_complete() {
    if [[ "$READLINE_LINE" == niko\ * ]]; then
        local query="${READLINE_LINE#niko }"
        local cmd=$(niko "$query" 2>/dev/null)
        if [ -n "$cmd" ]; then
            READLINE_LINE="$cmd"
            READLINE_POINT=${#cmd}
        fi
    fi
}
bind -x '"\t": _niko_complete'
`
	} else {
		fmt.Println("Unsupported shell. Manual setup required.")
		return nil
	}

	// Check if already installed
	content, err := os.ReadFile(rcFile)
	if err == nil && strings.Contains(string(content), "Niko") {
		fmt.Println("Already installed!")
		fmt.Println("\nUsage: niko list files<Tab>")
		return nil
	}

	// Append to rc file
	f, err := os.OpenFile(rcFile, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return err
	}
	defer f.Close()

	if _, err := f.WriteString(shellCode); err != nil {
		return err
	}

	fmt.Printf("Installed to %s\n\n", rcFile)
	fmt.Println("Restart your terminal, then:")
	fmt.Println("  niko list files<Tab>")

	return nil
}
