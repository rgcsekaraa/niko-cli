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

var (
	dim   = color.New(color.Faint)
	cyan  = color.New(color.FgCyan)
	green = color.New(color.FgGreen)
)

type InteractiveResult int

const (
	ResultRun InteractiveResult = iota
	ResultEdit
	ResultCancel
)

// InteractivePrompt shows the command and waits for user input
// Returns the action taken and the (possibly edited) command
func InteractivePrompt(command string) (InteractiveResult, string) {
	// Check if we're in a terminal
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		// Not interactive, just print and return
		fmt.Println(command)
		return ResultCancel, command
	}

	// Clean, minimal output - just the command with a run prompt
	fmt.Printf("\n  %s", command)
	dim.Print("  ⏎\n\n")

	// Read single keypress
	oldState, err := term.MakeRaw(int(os.Stdin.Fd()))
	if err != nil {
		fmt.Println(command)
		return ResultCancel, command
	}
	defer term.Restore(int(os.Stdin.Fd()), oldState)

	buf := make([]byte, 3)
	n, err := os.Stdin.Read(buf)
	if err != nil || n == 0 {
		return ResultCancel, command
	}

	// Restore terminal before any output
	term.Restore(int(os.Stdin.Fd()), oldState)

	switch {
	case buf[0] == 13 || buf[0] == 10: // Enter
		return ResultRun, command
	case buf[0] == 'e' || buf[0] == 'E' || buf[0] == 9: // 'e' or Tab
		edited := editCommand(command)
		if edited != "" {
			return ResultEdit, edited
		}
		return ResultCancel, command
	case buf[0] == 27 || buf[0] == 'q' || buf[0] == 'Q' || buf[0] == 3: // Esc, q, or Ctrl+C
		return ResultCancel, command
	default:
		// Any other key, treat as cancel
		return ResultCancel, command
	}
}

// editCommand lets the user edit the command with full readline support
func editCommand(command string) string {
	rl, err := readline.NewEx(&readline.Config{
		Prompt:          "  Edit: ",
		InterruptPrompt: "^C",
		EOFPrompt:       "exit",
	})
	if err != nil {
		return command
	}
	defer rl.Close()

	// Pre-fill the line with the command
	rl.WriteStdin([]byte(command))

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

	fmt.Println()
	green.Println("  Running...")
	fmt.Println()

	cmd := exec.Command(shell, "-c", command)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	return cmd.Run()
}
