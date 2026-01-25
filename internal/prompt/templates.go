package prompt

import (
	"fmt"
	"strings"
)

const SystemPrompt = `You are a helpful shell command generator. Output ONLY the command, nothing else.

%s

EXAMPLES:
"list files" → ls -la
"disk usage" → du -sh *
"how do i run ollama" → ollama serve
"how to start docker" → systemctl start docker
"run nginx" → nginx
"start redis" → redis-server
"run python script" → python script.py
"find py files" → find . -name "*.py"
"remove txt files" → rm *.txt
"git status" → git status
"ping google" → ping -c 4 google.com
"check memory" → free -h
"list processes" → ps aux

Command:`

const ContextTemplate = `SYSTEM INFO:
- OS: %s
- Architecture: %s
- Shell: %s
- Current directory: %s
- Available tools: %s`

func BuildSystemPrompt(ctx *SystemContext) string {
	contextInfo := fmt.Sprintf(ContextTemplate,
		ctx.OS,
		ctx.Arch,
		ctx.Shell,
		ctx.WorkingDir,
		strings.Join(ctx.AvailableTools, ", "),
	)

	return fmt.Sprintf(SystemPrompt, contextInfo)
}

func BuildUserPrompt(request string) string {
	return request
}
