use std::env;
use std::process::Command;

/// System context information for prompt generation
pub struct SystemContext {
    pub os: String,
    pub arch: String,
    pub shell: String,
    pub working_dir: String,
    pub available_tools: Vec<String>,
}

/// Gather system context (OS, shell, cwd, available tools)
pub fn gather_context() -> SystemContext {
    SystemContext {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        shell: detect_shell(),
        working_dir: env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into()),
        available_tools: detect_tools(),
    }
}

/// Build the system prompt for command generation mode
pub fn cmd_system_prompt(ctx: &SystemContext) -> String {
    let os_specific = match ctx.os.as_str() {
        "macos" => {
            r#"
OS-SPECIFIC NOTES (macOS):
- Use `open` instead of `xdg-open`
- `sed -i ''` needs empty string arg (BSD sed)
- Use `pbcopy`/`pbpaste` for clipboard
- `brew` is the primary package manager
- Use `caffeinate` to prevent sleep
- `dscacheutil -flushcache` to flush DNS
- `diskutil` instead of `fdisk`"#
        }
        "linux" => {
            r#"
OS-SPECIFIC NOTES (Linux):
- Use `xdg-open` to open files/URLs
- `sed -i` works directly (GNU sed)
- Use `xclip` or `xsel` for clipboard
- `systemctl` for service management
- `apt`/`dnf`/`pacman` depending on distro"#
        }
        "windows" => {
            r#"
OS-SPECIFIC NOTES (Windows):
- Use PowerShell syntax when possible
- `Start-Process` or `Invoke-Item` to open files
- `Set-Clipboard` / `Get-Clipboard` for clipboard
- `winget` or `choco` for package management
- Use backslashes for paths or quote forward slashes"#
        }
        _ => "",
    };

    format!(
        r#"You are an expert shell command generator for {os} ({arch}).

OUTPUT FORMAT:
- Output ONLY the executable command — no explanation, no markdown fences, no commentary
- For multi-step operations, use `&&` to chain commands
- For data pipelines, use `|` to pipe output
- Use `\` for line continuation only if the command would be extremely long

RULES:
1. The command MUST be immediately executable in {shell}
2. Use <PLACEHOLDER> syntax for values the user must substitute (API keys, usernames, URLs, paths)
3. Always prefer the safer/idiomatic approach for {os}:
   - Credential piping via stdin over CLI args (secrets visible in `ps`)
   - `--dry-run` or `echo` prefix for destructive exploratory commands
   - `-i` (interactive) flags where appropriate for destructive operations
4. If the user's request is ambiguous, generate the most common interpretation
5. If a tool from "Available tools" can do the job, prefer it over alternatives
6. For truly catastrophic commands (e.g., `rm -rf /`, format disk), output:
   echo "Declined: <specific reason>"
7. NEVER fabricate flags — only use flags you are certain exist for that tool
{os_specific}

SYSTEM:
- OS: {os}  |  Arch: {arch}  |  Shell: {shell}
- CWD: {cwd}
- Tools: {tools}

EXAMPLES — Files & Search:
"find large files over 100MB" → find . -type f -size +100M -exec ls -lh {{}} +
"find py files modified today" → find . -name "*.py" -mtime 0
"search for TODO in rust files" → grep -rn "TODO" --include="*.rs" .
"replace foo with bar in all js files" → find . -name "*.js" -exec sed -i 's/foo/bar/g' {{}} +
"count lines of code" → find . -name "*.rs" -o -name "*.py" | xargs wc -l | sort -n

EXAMPLES — Git & Version Control:
"git commits from last week" → git log --oneline --since="1 week ago"
"git squash last 3 commits" → git reset --soft HEAD~3 && git commit
"undo last commit keep changes" → git reset --soft HEAD~1
"git clone private repo with token" → git clone https://<TOKEN>@github.com/<OWNER>/<REPO>.git
"show git diff stats" → git diff --stat
"git stash with message" → git stash push -m "<MESSAGE>"
"cherry pick a commit" → git cherry-pick <COMMIT_HASH>

EXAMPLES — Docker & Containers:
"docker login github" → echo <GITHUB_PAT> | docker login ghcr.io -u <USERNAME> --password-stdin
"docker login gitlab" → echo <GITLAB_TOKEN> | docker login registry.gitlab.com -u oauth2 --password-stdin
"login to aws ecr" → aws ecr get-login-password --region <REGION> | docker login --username AWS --password-stdin <ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com
"docker cleanup everything" → docker system prune -af --volumes
"list docker images sorted by size" → docker images --format "{{{{.Repository}}}}:{{{{.Tag}}}} {{{{.Size}}}}" | sort -k2 -h
"run postgres in docker" → docker run -d --name postgres -p 5432:5432 -e POSTGRES_PASSWORD=<PASSWORD> postgres:16-alpine
"docker compose up detached" → docker compose up -d --build

EXAMPLES — Networking & SSH:
"ssh tunnel to remote db" → ssh -L 5432:localhost:5432 <USER>@<HOST> -N
"test if port is open" → nc -zv <HOST> <PORT>
"find process on port 3000" → lsof -i :3000
"kill process on port 3000" → lsof -ti:3000 | xargs kill -9
"download file" → curl -LO <URL>
"check ssl certificate" → openssl s_client -connect <HOST>:443 -servername <HOST> 2>/dev/null | openssl x509 -noout -dates
"generate ssh key" → ssh-keygen -t ed25519 -C "<EMAIL>"
"copy ssh key to server" → ssh-copy-id <USER>@<HOST>

EXAMPLES — Kubernetes:
"get failing pods" → kubectl get pods --field-selector=status.phase!=Running -A
"pod logs with follow" → kubectl logs -f <POD> -n <NAMESPACE>
"exec into pod" → kubectl exec -it <POD> -n <NAMESPACE> -- /bin/sh
"scale deployment" → kubectl scale deployment <NAME> --replicas=<N> -n <NAMESPACE>
"port forward" → kubectl port-forward svc/<SERVICE> <LOCAL_PORT>:<REMOTE_PORT> -n <NAMESPACE>

EXAMPLES — Database:
"postgres dump" → pg_dump -h <HOST> -U <USER> -d <DATABASE> -Fc > dump.sql
"postgres restore" → pg_restore -h <HOST> -U <USER> -d <DATABASE> dump.sql
"redis flush all" → redis-cli FLUSHALL
"mysql export" → mysqldump -h <HOST> -u <USER> -p <DATABASE> > dump.sql

EXAMPLES — System & Process:
"disk usage by directory" → du -sh */ | sort -rh | head -20
"memory usage" → free -h
"watch log file" → tail -f <LOGFILE>
"compress excluding dir" → tar czf archive.tar.gz --exclude='node_modules' --exclude='.git' <FOLDER>
"find and delete empty dirs" → find . -type d -empty -delete
"monitor process" → watch -n 1 'ps aux | grep <PROCESS>'

EXAMPLES — Cloud & CI:
"push docker image to ecr" → docker tag <IMAGE> <ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/<REPO>:<TAG> && docker push <ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/<REPO>:<TAG>
"terraform plan" → terraform plan -out=tfplan
"terraform apply" → terraform apply tfplan
"aws s3 sync" → aws s3 sync <LOCAL_DIR> s3://<BUCKET>/<PREFIX> --delete

Command:"#,
        os = ctx.os,
        arch = ctx.arch,
        shell = ctx.shell,
        cwd = ctx.working_dir,
        tools = ctx.available_tools.join(", "),
        os_specific = os_specific,
    )
}

/// Build the system prompt for explaining a command
#[allow(dead_code)]
pub fn cmd_explain_prompt() -> String {
    r#"You are a senior DevOps/systems engineer. The user will ask about a shell command or tool.

Provide a clear, structured explanation:

## What It Does
One-paragraph description of the command's purpose and behavior.

## Syntax
```
command [flags] [arguments]
```

## Key Flags
| Flag | Description |
|------|-------------|
| `-x` | What it does |

## Common Usage
```bash
# Example 1: description
command -flag arg

# Example 2: description
command -other-flag arg
```

## ⚠ Safety Notes
- Any destructive behavior, irreversible operations, or common gotchas

Be practical and concise. Omit sections that aren't relevant."#
        .to_string()
}

fn detect_shell() -> String {
    if cfg!(target_os = "windows") {
        if Command::new("pwsh").arg("--version").output().is_ok() {
            return "powershell".into();
        }
        return "cmd".into();
    }

    env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(String::from))
        .unwrap_or_else(|| "sh".into())
}

fn detect_tools() -> Vec<String> {
    let tools = [
        // Version control
        "git",
        "gh",
        "svn",
        // Containers
        "docker",
        "docker-compose",
        "podman",
        "kubectl",
        "helm",
        "k9s",
        "minikube",
        // Package managers
        "npm",
        "yarn",
        "pnpm",
        "bun",
        "pip",
        "pip3",
        "pipenv",
        "poetry",
        "go",
        "cargo",
        "brew",
        "apt",
        "dnf",
        "pacman",
        // Languages
        "python",
        "python3",
        "node",
        "deno",
        "ruby",
        "php",
        "java",
        // Build tools
        "make",
        "cmake",
        "mvn",
        "gradle",
        // Cloud
        "terraform",
        "ansible",
        "aws",
        "gcloud",
        "az",
        "flyctl",
        "vercel",
        // Databases
        "psql",
        "mysql",
        "mongo",
        "redis-cli",
        "sqlite3",
        // HTTP & networking
        "curl",
        "wget",
        "ssh",
        "scp",
        "rsync",
        "nc",
        "lsof",
        // Text & search
        "jq",
        "yq",
        "fzf",
        "rg",
        "fd",
        "awk",
        "sed",
        "grep",
        // Compression
        "tar",
        "zip",
        "unzip",
        "gzip",
        // System
        "htop",
        "top",
        "ps",
        "df",
        "du",
        // Media
        "ffmpeg",
        "convert",
    ];

    tools
        .iter()
        .filter(|tool| which(tool))
        .map(|s| s.to_string())
        .collect()
}

pub fn which(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tool Help Discovery
// ---------------------------------------------------------------------------

/// Maximum characters of --help output to include in prompt
const MAX_HELP_CHARS: usize = 3000;

/// Tools we skip --help for (builtins, shells, common unix tools the LLM already knows)
const SKIP_HELP: &[&str] = &[
    // Shells & builtins
    "sh", "bash", "zsh", "fish", "csh", "tcsh", "pwsh", "cmd", "cd", "echo", "export", "source",
    "alias", "true", "false", "yes", // File basics — the LLM knows these perfectly
    "cat", "ls", "cp", "mv", "rm", "mkdir", "rmdir", "touch", "ln", "chmod", "chown", "chgrp",
    "pwd", "whoami", "hostname", // Process basics
    "ps", "top", "htop", "df", "du", "free", "kill",
    // Text basics — the LLM knows these
    "head", "tail", "sort", "uniq", "wc", "tr", "cut", "paste", "tee", "xargs", "grep", "sed",
    "awk", "find", "diff", // Compression basics
    "tar", "gzip", "zip", "unzip", // These produce huge/unhelpful output
    "python", "python3", "node", "ruby", "php", "java",
];

/// Multi-subcommand tools that support `tool subcommand --help`
const SUBCOMMAND_TOOLS: &[&str] = &[
    "docker",
    "kubectl",
    "git",
    "cargo",
    "npm",
    "yarn",
    "pnpm",
    "bun",
    "pip",
    "pip3",
    "go",
    "terraform",
    "aws",
    "gcloud",
    "az",
    "helm",
    "flyctl",
    "vercel",
    "brew",
    "podman",
    "gh",
    "poetry",
    "pipenv",
    "minikube",
    "ansible",
];

/// Discover tool help for tools mentioned in the user's query.
///
/// Scans the query for words matching executables on PATH, runs their --help,
/// and returns formatted help text for the LLM. Also handles two-word
/// subcommands like "docker compose", "kubectl get".
pub fn discover_tool_help(query: &str, verbose: bool) -> String {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let mut help_sections = Vec::new();
    let mut seen_tools = std::collections::HashSet::new();

    // First pass: two-word subcommands (e.g., "docker compose", "kubectl get")
    for pair in words.windows(2) {
        let base = normalize_tool_word(pair[0]);
        let sub = normalize_tool_word(pair[1]);
        if base.is_empty() || sub.is_empty() {
            continue;
        }

        if SUBCOMMAND_TOOLS.contains(&base.as_str()) {
            let key = format!("{} {}", base, sub);
            if !seen_tools.contains(&key) && which(&base) {
                if let Some(help_text) = get_subcommand_help(&base, &sub) {
                    if verbose {
                        eprintln!(
                            "  [help] captured `{} {} --help` ({} chars)",
                            base,
                            sub,
                            help_text.len()
                        );
                    }
                    help_sections
                        .push(format!("TOOL REFERENCE: `{} {}`\n{}", base, sub, help_text));
                    seen_tools.insert(key);
                    seen_tools.insert(base.clone());
                }
            }
        }
    }

    // Second pass: single-word tools
    for word in &words {
        let tool = normalize_tool_word(word);
        if tool.is_empty() || tool.len() < 2 {
            continue;
        }
        if seen_tools.contains(&tool) {
            continue;
        }
        if SKIP_HELP.contains(&tool.as_str()) {
            continue;
        }

        if which(&tool) {
            if let Some(help_text) = get_tool_help(&tool) {
                if verbose {
                    eprintln!(
                        "  [help] captured `{} --help` ({} chars)",
                        tool,
                        help_text.len()
                    );
                }
                help_sections.push(format!("TOOL REFERENCE: `{}`\n{}", tool, help_text));
                seen_tools.insert(tool);
            }
        }
    }

    if help_sections.is_empty() {
        return String::new();
    }

    format!(
        "\n\nThe following --help output was captured from tools on this system. \
         Use ONLY the flags and syntax shown here — do NOT invent flags:\n\n{}",
        help_sections.join("\n\n---\n\n")
    )
}

/// Normalize a word from the query into a potential tool name
fn normalize_tool_word(word: &str) -> String {
    word.to_lowercase()
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.')
        .to_string()
}

/// Try --help, -h, then `tool help` to get help text
fn get_tool_help(tool: &str) -> Option<String> {
    if let Some(text) = run_help_command(tool, &["--help"]) {
        return Some(truncate_help(&text));
    }
    if let Some(text) = run_help_command(tool, &["-h"]) {
        return Some(truncate_help(&text));
    }
    if let Some(text) = run_help_command(tool, &["help"]) {
        return Some(truncate_help(&text));
    }
    None
}

/// Try `tool subcommand --help` or `tool help subcommand`
fn get_subcommand_help(tool: &str, subcommand: &str) -> Option<String> {
    if let Some(text) = run_help_command(tool, &[subcommand, "--help"]) {
        return Some(truncate_help(&text));
    }
    if let Some(text) = run_help_command(tool, &["help", subcommand]) {
        return Some(truncate_help(&text));
    }
    None
}

/// Run a command with args and capture output (picks stdout or stderr, whichever is longer)
fn run_help_command(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd)
        .args(args)
        // Kill after 3 seconds — some tools hang without a TTY
        .env("TERM", "dumb")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Some tools write help to stdout, others to stderr
    let text = if stdout.len() >= stderr.len() {
        stdout.to_string()
    } else {
        stderr.to_string()
    };

    let trimmed = text.trim();
    // Reject if too short or clearly an error
    if trimmed.len() < 30 {
        return None;
    }
    if trimmed.starts_with("error:") || trimmed.starts_with("Error:") {
        return None;
    }
    if trimmed.starts_with("command not found") {
        return None;
    }

    Some(trimmed.to_string())
}

/// Truncate help text to MAX_HELP_CHARS, breaking at a line boundary
fn truncate_help(text: &str) -> String {
    if text.len() <= MAX_HELP_CHARS {
        return text.to_string();
    }
    let truncated = &text[..MAX_HELP_CHARS];
    if let Some(last_nl) = truncated.rfind('\n') {
        format!("{}\n[...truncated]", &truncated[..last_nl])
    } else {
        format!("{}\n[...truncated]", truncated)
    }
}
