package cli

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"strings"

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

	fmt.Println()
	cyan.Printf("  %s\n", command)
	fmt.Println()
	dim.Println("  [Enter] Run  [e] Edit  [Esc/q] Cancel")
	fmt.Println()

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
		return ResultEdit, editCommand(command)
	case buf[0] == 27 || buf[0] == 'q' || buf[0] == 'Q' || buf[0] == 3: // Esc, q, or Ctrl+C
		dim.Println("  Cancelled")
		return ResultCancel, command
	default:
		// Any other key, treat as cancel
		return ResultCancel, command
	}
}

// editCommand lets the user edit the command inline
func editCommand(command string) string {
	fmt.Print("  Edit: ")

	// Pre-fill with the command using ANSI escape codes
	fmt.Print(command)

	// Move cursor to end and enable line editing
	reader := bufio.NewReader(os.Stdin)

	// Clear the pre-filled text and let user type
	fmt.Print("\r  Edit: ")

	line, err := reader.ReadString('\n')
	if err != nil {
		return command
	}

	edited := strings.TrimSpace(line)
	if edited == "" {
		return command
	}
	return edited
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
