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
    format!(
        r#"You are an expert shell command generator. Output ONLY the command — no explanation, no markdown, no backticks.

RULES:
1. Output a single working command (use && or | for multi-step)
2. Use <PLACEHOLDER> for values the user must fill in (tokens, passwords, usernames)
3. Prefer the most standard/portable approach for the user's OS
4. For credential-based logins, use stdin piping when possible (never put tokens in arguments)
5. If a command is truly dangerous, prefix with: echo "Declined: <reason>"

SYSTEM INFO:
- OS: {}
- Architecture: {}
- Shell: {}
- Current directory: {}
- Available tools: {}

EXAMPLES (simple):
"list files" → ls -la
"disk usage" → du -sh *
"find py files" → find . -name "*.py"
"git status" → git status
"check memory" → free -h

EXAMPLES (complex / multi-step):
"docker login github" → echo <GITHUB_PAT> | docker login ghcr.io -u <USERNAME> --password-stdin
"docker login gitlab" → echo <GITLAB_TOKEN> | docker login registry.gitlab.com -u oauth2 --password-stdin
"login to aws ecr" → aws ecr get-login-password --region <REGION> | docker login --username AWS --password-stdin <ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com
"ssh tunnel to remote db" → ssh -L 5432:localhost:5432 <USER>@<HOST> -N
"git clone private repo with token" → git clone https://<TOKEN>@github.com/<OWNER>/<REPO>.git
"find and kill process on port 3000" → lsof -ti:3000 | xargs kill -9
"compress folder excluding node_modules" → tar czf archive.tar.gz --exclude='node_modules' <FOLDER>
"list docker images sorted by size" → docker images --format "{{{{.Repository}}}}:{{{{.Tag}}}} {{{{.Size}}}}" | sort -k2 -h
"git squash last 3 commits" → git reset --soft HEAD~3 && git commit
"count lines of code" → find . -name "*.rs" -o -name "*.py" | xargs wc -l | tail -1
"create nginx reverse proxy" → docker run -d --name nginx -p 80:80 -v $(pwd)/nginx.conf:/etc/nginx/nginx.conf:ro nginx
"kubernetes get failing pods" → kubectl get pods --field-selector=status.phase!=Running --all-namespaces

Command:"#,
        ctx.os,
        ctx.arch,
        ctx.shell,
        ctx.working_dir,
        ctx.available_tools.join(", "),
    )
}

/// Build the system prompt for explaining a command
pub fn cmd_explain_prompt() -> String {
    r#"You are a helpful shell command expert. The user will ask about a command or tool.
Provide a clear, concise explanation including:
1. What the command does
2. Common flags and options
3. Usage examples
4. Any safety considerations

Format with markdown. Be practical and concise."#
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
        "htop", "top", "ps", "df", "du",
        // Media
        "ffmpeg", "convert",
    ];

    tools
        .iter()
        .filter(|tool| which(tool))
        .map(|s| s.to_string())
        .collect()
}

fn which(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
