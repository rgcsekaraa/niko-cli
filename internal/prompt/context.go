package prompt

import (
	"os"
	"os/exec"
	"runtime"
	"strings"
)

type SystemContext struct {
	OS             string
	Arch           string
	Shell          string
	WorkingDir     string
	AvailableTools []string
}

func GatherContext() *SystemContext {
	ctx := &SystemContext{
		OS:         runtime.GOOS,
		Arch:       runtime.GOARCH,
		Shell:      detectShell(),
		WorkingDir: getWorkingDir(),
	}

	ctx.AvailableTools = detectTools()

	return ctx
}

func detectShell() string {
	if runtime.GOOS == "windows" {
		if _, err := exec.LookPath("pwsh"); err == nil {
			return "powershell"
		}
		return "cmd"
	}

	if shell := os.Getenv("SHELL"); shell != "" {
		parts := strings.Split(shell, "/")
		return parts[len(parts)-1]
	}

	return "sh"
}

func getWorkingDir() string {
	dir, err := os.Getwd()
	if err != nil {
		return "unknown"
	}
	return dir
}

func detectTools() []string {
	toolsToCheck := []string{
		// Version control
		"git", "gh", "svn",
		// Containers
		"docker", "docker-compose", "podman", "kubectl", "helm", "k9s", "minikube",
		// Package managers
		"npm", "yarn", "pnpm", "bun", "pip", "pip3", "pipenv", "poetry",
		"go", "cargo", "brew", "apt", "dnf", "pacman",
		// Languages
		"python", "python3", "node", "deno", "ruby", "php", "java",
		// Build tools
		"make", "cmake", "mvn", "gradle",
		// Cloud
		"terraform", "ansible", "aws", "gcloud", "az", "flyctl", "vercel",
		// Databases
		"psql", "mysql", "mongo", "redis-cli", "sqlite3",
		// HTTP & networking
		"curl", "wget", "ssh", "scp", "rsync", "nc", "lsof",
		// Text & search
		"jq", "yq", "fzf", "rg", "fd", "awk", "sed", "grep",
		// Compression
		"tar", "zip", "unzip", "gzip",
		// System
		"htop", "top", "ps", "df", "du", "free",
		// Media
		"ffmpeg", "convert",
	}

	var available []string
	for _, tool := range toolsToCheck {
		if _, err := exec.LookPath(tool); err == nil {
			available = append(available, tool)
		}
	}

	return available
}

func GetOSHints() string {
	switch runtime.GOOS {
	case "darwin":
		return "macOS: use BSD-style flags (e.g., ls -G for colors)"
	case "linux":
		return "Linux: use GNU-style flags (e.g., ls --color for colors)"
	case "windows":
		return "Windows: prefer PowerShell cmdlets when appropriate"
	default:
		return ""
	}
}
