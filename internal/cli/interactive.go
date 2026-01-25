package cli

import (
	"fmt"
	"io"
	"os"
	"os/exec"

	"github.com/chzyer/readline"
	"golang.org/x/term"
)

type InteractiveResult int

const (
	ResultRun InteractiveResult = iota
	ResultCancel
)

// InteractivePrompt shows command in a shell-like prompt
// User can edit, press Enter to run, or Ctrl+C to cancel
func InteractivePrompt(command string) (InteractiveResult, string) {
	// Check if we're in a terminal
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		fmt.Println(command)
		return ResultCancel, command
	}

	rl, err := readline.NewEx(&readline.Config{
		Prompt:          "$ ",
		InterruptPrompt: "^C",
		EOFPrompt:       "",
	})
	if err != nil {
		fmt.Println(command)
		return ResultCancel, command
	}
	defer rl.Close()

	// Pre-fill with the command
	rl.WriteStdin([]byte(command))

	line, err := rl.Readline()
	if err != nil {
		if err == readline.ErrInterrupt || err == io.EOF {
			return ResultCancel, ""
		}
		return ResultCancel, command
	}

	if line == "" {
		return ResultCancel, ""
	}

	return ResultRun, line
}

// ExecuteCommand runs the command in the user's shell
func ExecuteCommand(command string) error {
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}

	fmt.Println()
	cmd := exec.Command(shell, "-c", command)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	return cmd.Run()
}
