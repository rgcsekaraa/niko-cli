package executor

import (
	"regexp"
	"strings"

	"github.com/niko-cli/niko/internal/config"
)

type RiskLevel int

const (
	Safe RiskLevel = iota
	Moderate
	Dangerous
	Critical
)

func (r RiskLevel) String() string {
	switch r {
	case Safe:
		return "safe"
	case Moderate:
		return "moderate"
	case Dangerous:
		return "dangerous"
	case Critical:
		return "critical"
	default:
		return "unknown"
	}
}

func (r RiskLevel) Description() string {
	switch r {
	case Safe:
		return "Read-only command, safe to execute"
	case Moderate:
		return "May modify files or state"
	case Dangerous:
		return "Could cause data loss or system changes"
	case Critical:
		return "EXTREMELY DANGEROUS - could destroy data or system"
	default:
		return "Unknown risk level"
	}
}

var safeCommands = []string{
	"ls", "ll", "la", "dir",
	"pwd", "cd",
	"cat", "less", "more", "head", "tail",
	"grep", "rg", "ag", "ack",
	"find", "fd", "locate",
	"echo", "printf",
	"date", "cal",
	"whoami", "id", "who", "w",
	"uname", "hostname",
	"env", "printenv",
	"which", "whereis", "type",
	"man", "help", "info",
	"wc", "sort", "uniq", "cut", "tr",
	"diff", "cmp",
	"file", "stat",
	"df", "du",
	"free", "top", "htop", "ps", "pgrep",
	"uptime", "lscpu", "lsmem",
	"ping", "host", "dig", "nslookup",
	"curl", "wget", "http",
	"git status", "git log", "git diff", "git branch", "git remote",
	"docker ps", "docker images", "docker logs",
	"kubectl get", "kubectl describe", "kubectl logs",
	"npm list", "npm outdated", "npm view",
	"pip list", "pip show",
	"go list", "go version", "go env",
	"cargo --version", "rustc --version",
	"node --version", "python --version",
}

var moderatePatterns = []string{
	`^git\s+(add|commit|stash|checkout|switch|merge)`,
	`^docker\s+(build|run|exec|start|stop)`,
	`^kubectl\s+(apply|create|delete|edit)`,
	`^npm\s+(install|update|uninstall)`,
	`^pip\s+(install|uninstall)`,
	`^go\s+(build|install|get|mod)`,
	`^cargo\s+(build|install|add|remove)`,
	`^mkdir`,
	`^touch`,
	`^cp\s`,
	`^mv\s`,
	`^ln\s`,
}

var dangerousPatterns = []string{
	`^rm\s`,
	`^rmdir`,
	`^git\s+(reset|rebase|push|force)`,
	`--force`,
	`--hard`,
	`-rf\s`,
	`^docker\s+(rm|rmi|prune|system\s+prune)`,
	`^kubectl\s+delete`,
	`^chmod`,
	`^chown`,
	`^sudo`,
	`^su\s`,
	`^kill`,
	`^pkill`,
	`^killall`,
	`>\s*[^|]`,
	`>>`,
}

var criticalPatterns = []string{
	`rm\s+(-rf?|--recursive).*(/|~|\$HOME|\*|\.\./)`,
	`rm\s+-rf?\s+/`,
	`rm\s+-rf?\s+\*`,
	`dd\s+if=`,
	`mkfs`,
	`fdisk`,
	`parted`,
	`:\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;`,
	`>\s*/dev/(s|h|v)d`,
	`chmod\s+(-R\s+)?777\s+/`,
	`chown\s+-R.*\s+/`,
	`wget.*\|\s*(ba)?sh`,
	`curl.*\|\s*(ba)?sh`,
	`\|\s*sh\s*$`,
	`\|\s*bash\s*$`,
}

func AssessRisk(command string) RiskLevel {
	command = strings.TrimSpace(command)
	cfg := config.Get()

	for _, blocked := range cfg.Safety.BlockedCommands {
		if strings.Contains(command, blocked) {
			return Critical
		}
	}

	for _, pattern := range criticalPatterns {
		if matched, _ := regexp.MatchString(pattern, command); matched {
			return Critical
		}
	}

	for _, pattern := range dangerousPatterns {
		if matched, _ := regexp.MatchString(pattern, command); matched {
			return Dangerous
		}
	}

	for _, pattern := range moderatePatterns {
		if matched, _ := regexp.MatchString(pattern, command); matched {
			return Moderate
		}
	}

	for _, safe := range safeCommands {
		if strings.HasPrefix(command, safe) {
			return Safe
		}
	}

	return Moderate
}

func ShouldAutoExecute(risk RiskLevel) bool {
	cfg := config.Get()

	switch cfg.Safety.AutoExecute {
	case "none":
		return false
	case "safe":
		return risk == Safe
	case "moderate":
		return risk <= Moderate
	case "all":
		return risk < Critical
	default:
		return risk == Safe
	}
}

func IsBlocked(command string) bool {
	cfg := config.Get()
	for _, blocked := range cfg.Safety.BlockedCommands {
		if strings.Contains(command, blocked) {
			return true
		}
	}
	return false
}
