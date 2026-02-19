use regex::Regex;

use crate::config;

/// Risk level of a shell command
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Safe => write!(f, "safe"),
            Self::Moderate => write!(f, "moderate"),
            Self::Dangerous => write!(f, "dangerous"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl RiskLevel {
    pub fn description(&self) -> &str {
        match self {
            Self::Safe => "Read-only command, safe to execute",
            Self::Moderate => "May modify files or state",
            Self::Dangerous => "Could cause data loss or system changes",
            Self::Critical => "EXTREMELY DANGEROUS — could destroy data or system",
        }
    }
}

const SAFE_COMMANDS: &[&str] = &[
    "ls",
    "ll",
    "la",
    "dir",
    "pwd",
    "cd",
    "cat",
    "less",
    "more",
    "head",
    "tail",
    "grep",
    "rg",
    "ag",
    "ack",
    "find",
    "fd",
    "locate",
    "echo",
    "printf",
    "date",
    "cal",
    "whoami",
    "id",
    "who",
    "w",
    "uname",
    "hostname",
    "env",
    "printenv",
    "which",
    "whereis",
    "type",
    "man",
    "help",
    "info",
    "wc",
    "sort",
    "uniq",
    "cut",
    "tr",
    "diff",
    "cmp",
    "file",
    "stat",
    "df",
    "du",
    "free",
    "top",
    "htop",
    "ps",
    "pgrep",
    "uptime",
    "lscpu",
    "lsmem",
    "ping",
    "host",
    "dig",
    "nslookup",
    "curl",
    "wget",
    "http",
    "git status",
    "git log",
    "git diff",
    "git branch",
    "git remote",
    "docker ps",
    "docker images",
    "docker logs",
    "kubectl get",
    "kubectl describe",
    "kubectl logs",
    "npm list",
    "npm outdated",
    "npm view",
    "pip list",
    "pip show",
    "go list",
    "go version",
    "go env",
    "cargo --version",
    "rustc --version",
    "node --version",
    "python --version",
];

const MODERATE_PATTERNS: &[&str] = &[
    r"^git\s+(add|commit|stash|checkout|switch|merge)",
    r"^docker\s+(build|run|exec|start|stop)",
    r"^kubectl\s+(apply|create|delete|edit)",
    r"^npm\s+(install|update|uninstall)",
    r"^pip\s+(install|uninstall)",
    r"^go\s+(build|install|get|mod)",
    r"^cargo\s+(build|install|add|remove)",
    r"^mkdir",
    r"^touch",
    r"^cp\s",
    r"^mv\s",
    r"^ln\s",
];

const DANGEROUS_PATTERNS: &[&str] = &[
    r"^rm\s",
    r"^rmdir",
    r"^git\s+(reset|rebase|push|force)",
    r"--force",
    r"--hard",
    r"-rf\s",
    r"^docker\s+(rm|rmi|prune|system\s+prune)",
    r"^kubectl\s+delete",
    r"^chmod",
    r"^chown",
    r"^sudo",
    r"^su\s",
    r"^kill",
    r"^pkill",
    r"^killall",
    r">\s*[^|]",
    r">>",
];

const CRITICAL_PATTERNS: &[&str] = &[
    r"rm\s+(-rf?|--recursive).*(\/|~|\$HOME|\*|\.\.\/)",
    r"rm\s+-rf?\s+\/",
    r"rm\s+-rf?\s+\*",
    r"dd\s+if=",
    r"mkfs",
    r"fdisk",
    r"parted",
    r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;",
    r">\s*\/dev\/(s|h|v)d",
    r"chmod\s+(-R\s+)?777\s+\/",
    r"chown\s+-R.*\s+\/",
    r"wget.*\|\s*(ba)?sh",
    r"curl.*\|\s*(ba)?sh",
    r"\|\s*sh\s*$",
    r"\|\s*bash\s*$",
];

/// Assess the risk level of a shell command
pub fn assess_risk(command: &str) -> RiskLevel {
    let command = command.trim();
    let cfg = config::get();

    // Check blocked commands first
    for blocked in &cfg.safety.blocked_commands {
        if command.contains(blocked.as_str()) {
            return RiskLevel::Critical;
        }
    }

    // Check critical patterns
    for pattern in CRITICAL_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(command) {
                return RiskLevel::Critical;
            }
        }
    }

    // Check dangerous patterns
    for pattern in DANGEROUS_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(command) {
                return RiskLevel::Dangerous;
            }
        }
    }

    // Check moderate patterns
    for pattern in MODERATE_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(command) {
                return RiskLevel::Moderate;
            }
        }
    }

    // Check safe commands
    for safe in SAFE_COMMANDS {
        if command.starts_with(safe) {
            return RiskLevel::Safe;
        }
    }

    RiskLevel::Moderate
}

/// Extract a clean command from LLM response
pub fn extract_command(response: &str) -> String {
    let mut response = response.trim().to_string();

    // Remove common prefixes
    for prefix in &["Command:", "command:", "CMD:", "cmd:", "$ ", "> "] {
        if let Some(stripped) = response.strip_prefix(prefix) {
            response = stripped.trim().to_string();
        }
    }

    // Extract from code blocks
    let code_block_re =
        Regex::new(r"```(?:bash|sh|shell|zsh|cmd|powershell)?\s*\n([\s\S]*?)\n```").unwrap();
    if let Some(caps) = code_block_re.captures(&response) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }

    // Extract from inline code
    let inline_re = Regex::new(r"`([^`]+)`").unwrap();
    if let Some(caps) = inline_re.captures(&response) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }

    // Take first valid-looking line
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Strip prompt markers
        let line = line
            .strip_prefix("$ ")
            .or_else(|| line.strip_prefix("> "))
            .or_else(|| line.strip_prefix("# "))
            .unwrap_or(line)
            .trim();

        if !line.is_empty() {
            return line.to_string();
        }
    }

    response
}

/// Check if a command is blocked
pub fn is_blocked(command: &str) -> bool {
    let cfg = config::get();
    cfg.safety
        .blocked_commands
        .iter()
        .any(|b| command.contains(b.as_str()))
}

/// Get the first tool/command from a command string
pub fn get_first_tool(command: &str) -> Option<String> {
    let command = command.trim();

    // Skip subshell / command substitution
    if command.starts_with('(') || command.starts_with('$') {
        return None;
    }

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let first = parts[0];

    // Skip paths
    if first.starts_with('/') || first.starts_with("./") || first.starts_with("../") {
        return None;
    }

    // Skip wrapper commands and recurse
    let wrappers = ["sudo", "env", "nohup", "time"];
    if wrappers.contains(&first) && parts.len() > 1 {
        let next = parts[1];
        if !is_operator(next) && !next.starts_with('-') {
            return get_first_tool(&parts[1..].join(" "));
        }
    }

    Some(first.to_string())
}

fn is_operator(s: &str) -> bool {
    matches!(
        s,
        "|" | "||" | "&&" | ">" | ">>" | "<" | "<<" | "2>" | "2>>" | "&>" | "&>>" | "1>" | "1>>"
    )
}

/// Check if a tool exists on the system
pub fn is_tool_available(tool: &str) -> bool {
    if tool.is_empty() {
        return true;
    }

    const BUILTINS: &[&str] = &[
        "echo", "cd", "pwd", "export", "source", "alias", "exit", "return", "set", "unset", "read",
        "eval", "exec", "trap", "wait", "kill", "test", "[", "[[", "if", "for", "while", "case",
        "function", "time",
    ];

    if BUILTINS.contains(&tool) {
        return true;
    }

    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
