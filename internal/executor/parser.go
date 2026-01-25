package executor

import (
	"os/exec"
	"regexp"
	"strings"
)

func ExtractCommand(response string) string {
	response = strings.TrimSpace(response)

	// Remove common prefixes the LLM might add
	prefixes := []string{"Command:", "command:", "CMD:", "cmd:", "$ ", "> "}
	for _, prefix := range prefixes {
		if strings.HasPrefix(response, prefix) {
			response = strings.TrimSpace(strings.TrimPrefix(response, prefix))
		}
	}

	codeBlockRe := regexp.MustCompile("```(?:bash|sh|shell|zsh|cmd|powershell)?\\s*\\n([\\s\\S]*?)\\n```")
	if matches := codeBlockRe.FindStringSubmatch(response); len(matches) > 1 {
		return strings.TrimSpace(matches[1])
	}

	inlineCodeRe := regexp.MustCompile("`([^`]+)`")
	if matches := inlineCodeRe.FindStringSubmatch(response); len(matches) > 1 {
		return strings.TrimSpace(matches[1])
	}

	lines := strings.Split(response, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		if strings.HasPrefix(line, "$") || strings.HasPrefix(line, ">") || strings.HasPrefix(line, "#") {
			line = strings.TrimPrefix(line, "$")
			line = strings.TrimPrefix(line, ">")
			line = strings.TrimPrefix(line, "#")
			return strings.TrimSpace(line)
		}
	}

	if len(lines) > 0 {
		firstLine := strings.TrimSpace(lines[0])
		if looksLikeCommand(firstLine) {
			return firstLine
		}
	}

	return response
}

func looksLikeCommand(s string) bool {
	if len(s) == 0 || len(s) > 500 {
		return false
	}

	if strings.HasPrefix(s, "I ") ||
		strings.HasPrefix(s, "The ") ||
		strings.HasPrefix(s, "This ") ||
		strings.HasPrefix(s, "To ") ||
		strings.HasPrefix(s, "You ") ||
		strings.HasPrefix(s, "Here ") {
		return false
	}

	commandPrefixes := []string{
		"ls", "cd", "pwd", "cat", "echo", "grep", "find", "mkdir", "rm", "cp", "mv",
		"git", "docker", "kubectl", "npm", "yarn", "pip", "go", "cargo", "make",
		"curl", "wget", "ssh", "scp", "tar", "zip", "unzip", "chmod", "chown",
		"ps", "kill", "top", "df", "du", "head", "tail", "sort", "uniq", "wc",
		"awk", "sed", "cut", "tr", "diff", "touch", "ln", "file", "which",
		"python", "python3", "node", "ruby", "perl", "java", "javac",
		"brew", "apt", "apt-get", "yum", "dnf", "pacman",
		"sudo", "su", "env", "export", "source", "alias",
	}

	for _, prefix := range commandPrefixes {
		if strings.HasPrefix(s, prefix+" ") || s == prefix {
			return true
		}
	}

	return false
}

func SplitCommands(command string) []string {
	var commands []string

	parts := strings.Split(command, "&&")
	for _, part := range parts {
		subParts := strings.Split(part, ";")
		for _, subPart := range subParts {
			cmd := strings.TrimSpace(subPart)
			if cmd != "" {
				commands = append(commands, cmd)
			}
		}
	}

	return commands
}

// GetFirstTool extracts the first command/tool from a command string
func GetFirstTool(command string) string {
	command = strings.TrimSpace(command)

	// Handle subshell/command substitution at start
	if strings.HasPrefix(command, "(") || strings.HasPrefix(command, "$") {
		return ""
	}

	// Get the first word
	parts := strings.Fields(command)
	if len(parts) == 0 {
		return ""
	}

	firstWord := parts[0]

	// Skip wrapper commands only if followed by another command (not a pipe or flag)
	skipPrefixes := []string{"sudo", "env", "nohup", "time"}
	for _, prefix := range skipPrefixes {
		if firstWord == prefix && len(parts) > 1 {
			nextWord := parts[1]
			// Don't skip if next is a pipe, redirect, or flag
			if nextWord == "|" || nextWord == ">" || nextWord == ">>" ||
				nextWord == "<" || nextWord == "&&" || nextWord == "||" ||
				strings.HasPrefix(nextWord, "-") {
				return firstWord
			}
			// Skip to next word
			return GetFirstTool(strings.Join(parts[1:], " "))
		}
	}

	return firstWord
}

// IsToolAvailable checks if a tool is available in PATH
func IsToolAvailable(tool string) bool {
	if tool == "" {
		return true
	}

	// Built-in shell commands that don't need to be in PATH
	builtins := map[string]bool{
		"echo": true, "cd": true, "pwd": true, "export": true,
		"source": true, "alias": true, "exit": true, "return": true,
		"set": true, "unset": true, "read": true, "eval": true,
		"exec": true, "trap": true, "wait": true, "kill": true,
		"test": true, "[": true, "[[": true, "if": true, "for": true,
		"while": true, "case": true, "function": true, "time": true,
	}

	if builtins[tool] {
		return true
	}

	_, err := exec.LookPath(tool)
	return err == nil
}
