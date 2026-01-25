package executor

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"
)

type Result struct {
	Command  string
	ExitCode int
	Stdout   string
	Stderr   string
	Error    error
}

type Runner struct {
	shell     string
	shellFlag string
}

func NewRunner() *Runner {
	shell, flag := detectShell()
	return &Runner{
		shell:     shell,
		shellFlag: flag,
	}
}

func detectShell() (string, string) {
	if runtime.GOOS == "windows" {
		if _, err := exec.LookPath("pwsh"); err == nil {
			return "pwsh", "-Command"
		}
		return "cmd", "/C"
	}

	if shell := os.Getenv("SHELL"); shell != "" {
		return shell, "-c"
	}

	for _, sh := range []string{"zsh", "bash", "sh"} {
		if path, err := exec.LookPath(sh); err == nil {
			return path, "-c"
		}
	}

	return "sh", "-c"
}

func (r *Runner) Run(ctx context.Context, command string) *Result {
	result := &Result{
		Command: command,
	}

	if IsBlocked(command) {
		result.ExitCode = 1
		result.Error = fmt.Errorf("command is blocked by safety settings")
		return result
	}

	cmd := exec.CommandContext(ctx, r.shell, r.shellFlag, command)
	cmd.Dir, _ = os.Getwd()

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	cmd.Env = os.Environ()

	err := cmd.Run()
	result.Stdout = stdout.String()
	result.Stderr = stderr.String()

	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			result.ExitCode = exitErr.ExitCode()
		} else {
			result.ExitCode = 1
		}
		result.Error = err
	}

	return result
}

func (r *Runner) RunInteractive(ctx context.Context, command string) error {
	if IsBlocked(command) {
		return fmt.Errorf("command is blocked by safety settings")
	}

	cmd := exec.CommandContext(ctx, r.shell, r.shellFlag, command)
	cmd.Dir, _ = os.Getwd()
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = os.Environ()

	return cmd.Run()
}

func (r *Runner) DryRun(command string) string {
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("Shell: %s\n", r.shell))
	sb.WriteString(fmt.Sprintf("Command: %s %s '%s'\n", r.shell, r.shellFlag, command))
	sb.WriteString(fmt.Sprintf("Working directory: %s\n", getCurrentDir()))
	sb.WriteString(fmt.Sprintf("Risk level: %s\n", AssessRisk(command)))
	return sb.String()
}

func getCurrentDir() string {
	dir, err := os.Getwd()
	if err != nil {
		return "unknown"
	}
	return dir
}

func (r *Runner) GetShell() string {
	return r.shell
}
