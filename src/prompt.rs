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
        r#"You are a helpful shell command generator. Output ONLY the command, nothing else.

SYSTEM INFO:
- OS: {}
- Architecture: {}
- Shell: {}
- Current directory: {}
- Available tools: {}

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
