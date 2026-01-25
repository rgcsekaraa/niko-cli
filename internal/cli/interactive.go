package cli

import (
	"fmt"
	"io"
	"os"
	"os/exec"

	"github.com/chzyer/readline"
	"github.com/fatih/color"
	"golang.org/x/term"
)

var dim = color.New(color.Faint)

// InteractivePrompt shows the command and waits for user action
func InteractivePrompt(command string) (bool, string) {
	// Check if we're in a terminal
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		fmt.Println(command)
		return false, command
	}

	// Show command and options
	fmt.Printf("\n%s\n\n", command)
	dim.Println("[Tab] edit   [Enter] run   [Ctrl+C] cancel")
	fmt.Println()

	// Read single keypress
	oldState, err := term.MakeRaw(int(os.Stdin.Fd()))
	if err != nil {
		return false, command
	}
	defer term.Restore(int(os.Stdin.Fd()), oldState)

	buf := make([]byte, 3)
	n, err := os.Stdin.Read(buf)
	if err != nil || n == 0 {
		return false, command
	}

	// Restore terminal before any output
	term.Restore(int(os.Stdin.Fd()), oldState)

	switch {
	case buf[0] == 13 || buf[0] == 10: // Enter
		return true, command
	case buf[0] == 9: // Tab - edit mode
		edited := editCommand(command)
		if edited != "" {
			return true, edited
		}
		return false, ""
	default: // Ctrl+C or any other key
		return false, ""
	}
}

// editCommand opens readline for editing
func editCommand(command string) string {
	// Move cursor up and clear lines
	fmt.Print("\033[A\033[2K\033[A\033[2K\033[A\033[2K\033[A\033[2K")

	rl, err := readline.NewEx(&readline.Config{
		Prompt:          "$ ",
		InterruptPrompt: "^C",
	})
	if err != nil {
		return command
	}
	defer rl.Close()

	// Set buffer with command (pre-fill)
	rl.Operation.SetBuffer(command)

	line, err := rl.Readline()
	if err != nil {
		if err == readline.ErrInterrupt || err == io.EOF {
			return ""
		}
		return command
	}

	if line == "" {
		return command
	}
	return line
}

// ExecuteCommand runs the command in the user's shell
func ExecuteCommand(command string) error {
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}

	cmd := exec.Command(shell, "-c", command)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	// Run and ignore exit errors (command output already shown)
	cmd.Run()
	return nil
}
