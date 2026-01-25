package prompt

import (
	"fmt"
	"strings"
)

const SystemPrompt = `Convert the request to a shell command. Output ONLY the command.

%s

EXAMPLES:
"list files" → ls -la
"disk usage" → du -sh *
"run ollama" → ollama serve
"start docker" → docker start
"find py files" → find . -name "*.py"
"remove txt files" → rm *.txt
"git status" → git status
"ping google" → ping -c 4 google.com

DECLINE ONLY these exact patterns (output: echo "Declined"):
- rm -rf / or rm -rf /*
- dd if=/dev/zero of=/dev
- :(){ :|:& };:

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
