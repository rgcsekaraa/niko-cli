use std::env;
use std::process::Command;
use std::sync::OnceLock;

/// System context information for prompt generation
#[derive(Clone)]
pub struct SystemContext {
    pub os: String,
    pub arch: String,
    pub shell: String,
    pub working_dir: String,
    pub available_tools: Vec<String>,
}

static TOOL_CACHE: OnceLock<Vec<String>> = OnceLock::new();

/// Gather system context (OS, shell, cwd, available tools)
pub fn gather_context() -> SystemContext {
    SystemContext {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        shell: detect_shell(),
        working_dir: env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into()),
        available_tools: TOOL_CACHE.get_or_init(detect_tools).clone(),
    }
}
/// Build the system prompt for the chat assistant
pub fn chat_system_prompt(ctx: &SystemContext) -> String {
    format!(
        r#"You are Niko, an expert AI programming assistant running directly in the user's terminal.
Your goal is to provide concise, accurate, and immediately actionable answers.

CURRENT SYSTEM CONTEXT:
- OS: {os}
- Architecture: {arch}
- Shell: {shell}
- Working Directory: {cwd}
- Available Tools on PATH: {tools}

RULES:
1. Provide extremely accurate code blocks and shell commands when requested.
2. If given a file path, assume it is relative to the current working directory.
3. Keep responses concise unless asked for a detailed explanation.
4. Prefer using the listed available tools if applicable to the user's request.
5. Use markdown formatting heavily for readability. Always specify the language for code blocks.
6. When explaining code, be structured and point out potential bugs or missing edge cases."#,
        os = ctx.os,
        arch = ctx.arch,
        shell = ctx.shell,
        cwd = ctx.working_dir,
        tools = ctx.available_tools.join(", "),
    )
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
    // Find a safe char boundary at or before MAX_HELP_CHARS
    let mut end = MAX_HELP_CHARS;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    if let Some(last_nl) = truncated.rfind('\n') {
        format!("{}\n[...truncated]", &truncated[..last_nl])
    } else {
        format!("{}\n[...truncated]", truncated)
    }
}
