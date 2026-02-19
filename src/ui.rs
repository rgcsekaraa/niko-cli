use std::io::{self, BufRead, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use colored::*;

// ─── Box-drawing constants (Claude Code-inspired) ───────────────────────────

const BOX_TL: &str = "╭";
const BOX_TR: &str = "╮";
const BOX_BL: &str = "╰";
const BOX_BR: &str = "╯";
const BOX_H: &str = "─";
const BOX_V: &str = "│";
const BOX_SEP_L: &str = "├";
const BOX_SEP_R: &str = "┤";

/// Width of the TUI box (excluding border characters)
const BOX_WIDTH: usize = 62;

// ─── Box drawing helpers ────────────────────────────────────────────────────

/// Draw a top border with an optional title
pub fn box_top(title: &str) {
    if title.is_empty() {
        eprintln!("{}{}{}", BOX_TL.dimmed(), BOX_H.repeat(BOX_WIDTH).dimmed(), BOX_TR.dimmed());
    } else {
        let title_display = format!(" {} ", title);
        let title_plain_len = strip_ansi_len(&title_display);
        let padding = if BOX_WIDTH > title_plain_len + 2 {
            BOX_WIDTH - title_plain_len - 2
        } else {
            0
        };
        eprintln!(
            "{}{}{}{}{}",
            BOX_TL.dimmed(),
            BOX_H.repeat(2).dimmed(),
            title_display,
            BOX_H.repeat(padding).dimmed(),
            BOX_TR.dimmed()
        );
    }
}

/// Draw a bottom border
pub fn box_bottom() {
    eprintln!("{}{}{}", BOX_BL.dimmed(), BOX_H.repeat(BOX_WIDTH).dimmed(), BOX_BR.dimmed());
}

/// Draw a separator line
pub fn box_sep() {
    eprintln!("{}{}{}", BOX_SEP_L.dimmed(), BOX_H.repeat(BOX_WIDTH).dimmed(), BOX_SEP_R.dimmed());
}

/// Draw an empty line inside a box
pub fn box_empty() {
    eprintln!("{}{}{}", BOX_V.dimmed(), " ".repeat(BOX_WIDTH), BOX_V.dimmed());
}

/// Draw a line inside a box with content (left-aligned with 2-char indent)
pub fn box_line(content: &str) {
    let plain_len = strip_ansi_len(content);
    let padding = if BOX_WIDTH > plain_len + 2 {
        BOX_WIDTH - plain_len - 2
    } else {
        0
    };
    eprintln!("{} {}{}{}", BOX_V.dimmed(), content, " ".repeat(padding), BOX_V.dimmed());
}

/// Draw a key-value line inside a box
pub fn box_kv(key: &str, value: &str) {
    let formatted = format!("{} {}", key.dimmed(), value);
    box_line(&formatted);
}

/// Draw a key-value line with the key bold
pub fn box_kv_bold(key: &str, value: &str) {
    let formatted = format!("{} {}", key.bold(), value);
    box_line(&formatted);
}

// ─── Themed output ──────────────────────────────────────────────────────────

/// Print an error message
pub fn print_error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg.red());
}

/// Print a warning message
pub fn print_warning(msg: &str) {
    eprintln!("{} {}", "!".yellow().bold(), msg.yellow());
}

/// Print a dim/subtle message (used for hints)
pub fn print_dim(msg: &str) {
    eprintln!("{}", msg.dimmed());
}

/// Print a success message
pub fn print_success(msg: &str) {
    eprintln!("{} {}", "✓".green().bold(), msg);
}

// ─── Spinner ────────────────────────────────────────────────────────────────

pub struct Spinner {
    message: String,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl Spinner {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn start(&mut self) {
        if !atty_is_terminal() {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let message = self.message.clone();

        self.handle = Some(std::thread::spawn(move || {
            let mut i = 0;
            while running.load(Ordering::SeqCst) {
                eprint!(
                    "\r{} {} {}",
                    BOX_V.dimmed(),
                    SPINNER_FRAMES[i % SPINNER_FRAMES.len()].cyan(),
                    message.dimmed()
                );
                let _ = io::stderr().flush();
                i += 1;
                std::thread::sleep(Duration::from_millis(80));
            }
            eprint!("\r\x1b[K");
            let _ = io::stderr().flush();
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Update the spinner message
    pub fn set_message(&mut self, msg: &str) {
        self.message = msg.to_string();
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn atty_is_terminal() -> bool {
    unsafe { libc_isatty(2) != 0 }
}

extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

// ─── Clipboard ──────────────────────────────────────────────────────────────

pub fn copy_to_clipboard(text: &str) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => clipboard.set_text(text).is_ok(),
        Err(_) => false,
    }
}

// ─── Stdin reader with live line counter ────────────────────────────────────

/// Check if stdin is a terminal (not piped)
fn stdin_is_terminal() -> bool {
    unsafe { libc_isatty(0) != 0 }
}

/// Read all input from stdin — handles both piped input and interactive paste.
/// For interactive mode: shows a live "[N lines pasted]" counter.
/// For piped input: reads everything then shows a summary.
pub fn read_stdin_input() -> io::Result<String> {
    let stdin = io::stdin();

    // Piped input — read all, then show summary
    if !stdin_is_terminal() {
        let mut buf = String::new();
        stdin.lock().read_to_string(&mut buf)?;
        let line_count = buf.lines().count();
        if line_count > 0 {
            eprintln!(
                "{} {} {}",
                BOX_V.dimmed(),
                format!("[{} lines received]", line_count).cyan(),
                "via pipe".dimmed()
            );
        }
        return Ok(buf);
    }

    // Interactive mode — live line counter
    eprintln!();
    box_top(&"Paste Code".bold().to_string());
    box_line(&"Paste your code below. Press Ctrl-D or two empty lines to finish.".dimmed().to_string());
    box_sep();

    let mut lines = Vec::new();
    let mut consecutive_empty = 0;
    let reader = stdin.lock();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            consecutive_empty += 1;
            if consecutive_empty >= 2 {
                break;
            }
            lines.push(line);
        } else {
            consecutive_empty = 0;
            lines.push(line);
        }

        // Update live counter on stderr (overwrite the same line)
        eprint!(
            "\r\x1b[K{} {}",
            BOX_V.dimmed(),
            format!("[{} lines pasted]", lines.len()).cyan()
        );
        let _ = io::stderr().flush();
    }

    // Clear the counter line and show final summary
    eprint!("\r\x1b[K");

    // Remove trailing empty lines
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }

    box_line(&format!("{}", format!("[{} lines pasted]", lines.len()).green().bold()));
    box_bottom();
    eprintln!();

    Ok(lines.join("\n"))
}

// ─── Code preview (collapsible) ─────────────────────────────────────────────

/// Show a compact code preview with "[N lines] — press Enter to expand"
/// Returns true if the user chose to expand
pub fn show_code_preview(code: &str) -> bool {
    let lines: Vec<&str> = code.lines().collect();
    let count = lines.len();

    if count <= 20 {
        // Short code — show it all in a box
        box_top(&format!("{}", format!("Code ({} lines)", count).dimmed()));
        for line in &lines {
            let truncated = if line.len() > BOX_WIDTH - 3 {
                format!("{}…", &line[..BOX_WIDTH - 4])
            } else {
                line.to_string()
            };
            box_line(&truncated.dimmed().to_string());
        }
        box_bottom();
        return false;
    }

    // Large code — show preview with first 5 and last 3 lines
    box_top(&format!("{}", format!("Code ({} lines)", count).dimmed()));

    // First 5 lines
    for line in lines.iter().take(5) {
        let truncated = if line.len() > BOX_WIDTH - 3 {
            format!("{}…", &line[..BOX_WIDTH - 4])
        } else {
            line.to_string()
        };
        box_line(&truncated.dimmed().to_string());
    }

    // Collapsed section
    let hidden = count - 8; // 5 top + 3 bottom
    box_line(
        &format!(
            "   {} {}",
            format!("⋯ {} lines hidden", hidden).dimmed(),
            "(press Enter to expand, or any key + Enter to continue)".dimmed()
        )
    );

    // Last 3 lines
    for line in lines.iter().skip(count - 3) {
        let truncated = if line.len() > BOX_WIDTH - 3 {
            format!("{}…", &line[..BOX_WIDTH - 4])
        } else {
            line.to_string()
        };
        box_line(&truncated.dimmed().to_string());
    }

    box_bottom();

    // Prompt for expansion
    eprint!("{}", "  Press Enter to expand, or any key to continue: ".dimmed());
    let _ = io::stderr().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        if input.trim().is_empty() {
            // Expand — show all lines
            eprintln!();
            box_top(&format!("{}", format!("Code ({} lines) — expanded", count).dimmed()));
            for (i, line) in lines.iter().enumerate() {
                let line_num = format!("{:>4}", i + 1).dimmed();
                let truncated = if line.len() > BOX_WIDTH - 8 {
                    format!("{}…", &line[..BOX_WIDTH - 9])
                } else {
                    line.to_string()
                };
                box_line(&format!("{} {}", line_num, truncated.dimmed()));
            }
            box_bottom();
            return true;
        }
    }

    false
}

// ─── Explanation display (Claude Code-inspired) ─────────────────────────────

/// Display a formatted explanation result using box drawing
pub fn display_explanation(result: &crate::chunker::ExplainResult) {
    eprintln!();

    // Header box
    box_top(&format!("{}", "Explanation".bold()));

    let stats = format!(
        "{} lines analyzed{}",
        result.total_lines.to_string().cyan(),
        if result.total_chunks > 1 {
            format!("  •  {} chunks processed", result.total_chunks.to_string().cyan())
        } else {
            String::new()
        }
    );
    box_line(&stats);
    box_bottom();
    eprintln!();

    // Overall summary
    if result.total_chunks > 1 {
        println!("{}", "  Overview".bold());
        println!("{}", "  ────────".dimmed());
        for line in result.overall_summary.lines() {
            println!("  {}", line);
        }
        println!();

        // Per-chunk details in a box
        box_top(&format!("{}", "Detailed Analysis".dimmed()));
        for (i, chunk) in result.chunk_explanations.iter().enumerate() {
            if i > 0 {
                box_sep();
            }
            box_line(&format!(
                "{}",
                format!("Lines {}-{}", chunk.start_line, chunk.end_line).bold()
            ));
            box_empty();
            for line in chunk.explanation.lines() {
                let display = if line.len() > BOX_WIDTH - 4 {
                    format!("{}…", &line[..BOX_WIDTH - 5])
                } else {
                    line.to_string()
                };
                box_line(&format!("  {}", display));
            }
        }
        box_bottom();
    } else {
        for line in result.overall_summary.lines() {
            println!("  {}", line);
        }
    }
    println!();

    // Follow-up questions
    if !result.follow_up_questions.is_empty() {
        box_top(&format!("{}", "Follow-up Questions".dimmed()));
        for (i, q) in result.follow_up_questions.iter().enumerate() {
            box_line(&format!(
                "{}  {}",
                format!("{}.", i + 1).cyan(),
                q
            ));
        }
        box_bottom();
        eprintln!();
    }
}

// ─── Command output (Claude Code-inspired) ──────────────────────────────────

/// Display a generated command in a box
pub fn display_command(command: &str) {
    eprintln!();
    box_top(&format!("{}", "Command".dimmed()));
    box_empty();

    // Handle multi-line commands
    for line in command.lines() {
        box_line(&format!("  {}", line.white().bold()));
    }

    box_empty();
    box_bottom();
}

// ─── Prompt input ───────────────────────────────────────────────────────────

pub fn prompt_input(prompt: &str) -> io::Result<String> {
    eprint!("{}", prompt);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim_end_matches('\n').trim_end_matches('\r').to_string())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Get the display length of a string, stripping ANSI escape codes
fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            len += 1;
        }
    }
    len
}
