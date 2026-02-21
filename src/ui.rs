use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crossterm::terminal;

use colored::*;

use unicode_width::UnicodeWidthStr;

// ─── Box-drawing constants (Claude Code-inspired) ───────────────────────────

const BOX_TL: &str = "╭";
const BOX_TR: &str = "╮";
const BOX_BL: &str = "╰";
const BOX_BR: &str = "╯";
const BOX_H: &str = "─";
const BOX_V: &str = "│";
const BOX_SEP_L: &str = "├";
const BOX_SEP_R: &str = "┤";

/// Print a horizontal rule to the terminal
pub fn print_rule() {
    let width = get_box_width();
    println!("{}", BOX_H.repeat(width + 2).dimmed());
}
/// Minimum width for the TUI box
const MIN_BOX_WIDTH: usize = 62;
/// Maximum width for the TUI box (to prevent it becoming too sparse on ultra-wide screens)
const MAX_BOX_WIDTH: usize = 100;

/// Get the current terminal width with a small margin
pub fn get_box_width() -> usize {
    if let Ok((w, _)) = terminal::size() {
        // Use full width minus margin, clamped between MIN and MAX
        let target = (w as usize).saturating_sub(4);
        target.clamp(MIN_BOX_WIDTH, MAX_BOX_WIDTH)
    } else {
        MIN_BOX_WIDTH
    }
}

// ─── Box drawing helpers ────────────────────────────────────────────────────

/// Draw a top border with an optional title
pub fn box_top(title: &str) {
    let width = get_box_width();
    let sanitized_title = sanitize_text(title);
    let title = &sanitized_title;
    if title.is_empty() {
        eprintln!(
            "{}{}{}",
            BOX_TL.dimmed(),
            BOX_H.repeat(width).dimmed(),
            BOX_TR.dimmed()
        );
    } else {
        let title_display = format!(" {} ", title);
        let title_plain_len = strip_ansi_len(&title_display);
        let padding = if width > title_plain_len + 2 {
            width - title_plain_len - 2
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
    let width = get_box_width();
    eprintln!(
        "{}{}{}",
        BOX_BL.dimmed(),
        BOX_H.repeat(width).dimmed(),
        BOX_BR.dimmed()
    );
}

/// Draw a separator line
pub fn box_sep() {
    let width = get_box_width();
    eprintln!(
        "{}{}{}",
        BOX_SEP_L.dimmed(),
        BOX_H.repeat(width).dimmed(),
        BOX_SEP_R.dimmed()
    );
}

/// Draw an empty line inside a box
pub fn box_empty() {
    let width = get_box_width();
    eprintln!(
        "{}{}{}",
        BOX_V.dimmed(),
        " ".repeat(width),
        BOX_V.dimmed()
    );
}

pub fn sanitize_text(text: &str) -> String {
    // Replace em-dashes with standard hyphens as a safeguard mechanism 
    // against multibyte character length calculation issues in older terminals
    text.replace('—', "-")
}

/// Draw a line inside a box with content (left-aligned with 2-char indent)
/// Content that exceeds BOX_WIDTH is automatically truncated with "…"
pub fn box_line(content: &str) {
    let width = get_box_width();
    let sanitized_content = sanitize_text(content);
    let content = &sanitized_content;
    
    // Available width for content: width minus the leading space (1 char) and trailing space (1 char)
    let max_content_width = width - 2;
    let plain_len = strip_ansi_len(content);

    let (display_content, display_len) = if plain_len > max_content_width {
        let truncated = truncate_ansi(content, max_content_width.saturating_sub(1));
        let tlen = strip_ansi_len(&truncated);
        (format!("{}…", truncated), tlen + 1)
    } else {
        (content.to_string(), plain_len)
    };

    let padding = if max_content_width > display_len {
        max_content_width - display_len
    } else {
        0
    };
    eprintln!(
        "{} {} {}{}",
        BOX_V.dimmed(),
        display_content,
        " ".repeat(padding),
        BOX_V.dimmed()
    );
}

/// Draw a key-value line inside a box
pub fn box_kv(key: &str, value: &str) {
    let key = sanitize_text(key);
    let value = sanitize_text(value);
    let formatted = format!("{} {}", key.dimmed(), value);
    box_line(&formatted);
}

/// Draw a key-value line with the key bold
pub fn box_kv_bold(key: &str, value: &str) {
    let key = sanitize_text(key);
    let value = sanitize_text(value);
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
    #[allow(dead_code)]
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
    io::stderr().is_terminal()
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
    io::stdin().is_terminal()
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
    box_line(
        &"Paste your code below. Press Ctrl-D or two empty lines to finish."
            .dimmed()
            .to_string(),
    );
    box_sep();

    let mut lines = Vec::new();
    let reader = stdin.lock();
    let mut empty_count = 0;

    for line in reader.lines() {
        let line = line?;
        
        // Two consecutive empty lines to finish (common CLI pattern for multi-line input)
        if line.trim().is_empty() {
            empty_count += 1;
            if empty_count >= 2 {
                break;
            }
        } else {
            empty_count = 0;
        }
        
        lines.push(line);

        // Live counter shows status during paste
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
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    box_line(&format!(
        "{}",
        format!("[{} lines pasted]", lines.len()).green().bold()
    ));
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
    if count <= 5 {
        // Very short code - show it all
        box_top(&format!("{}", format!("Code ({} lines)", count).dimmed()));
        for line in &lines {
            box_line(&line.dimmed().to_string());
        }
        box_bottom();
        return false;
    }

    // Default: collapse for anything > 5 lines
    box_top(&format!("{}", format!("Code ({} lines)", count).dimmed()));
    box_line(&lines[0].white().to_string());
    if count > 2 {
        box_line(&"".to_string());
        box_line(&format!(
            "   \u{22ef} {} lines hidden (press Enter to expand, or any key to continue)",
            count - 2
        ).dimmed().to_string());
        box_line(&"".to_string());
    }
    if count > 1 {
        box_line(&lines[count - 1].white().to_string());
    }
    box_bottom();

    // Prompt for expansion
    eprint!(
        "{}",
        "  Press Enter to expand, or any key to continue: ".dimmed()
    );
    let _ = io::stderr().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() && input.trim().is_empty() {
        // Expand — show all lines
        eprintln!();
        box_top(&format!(
            "{}",
            format!("Code ({} lines) — expanded", count).dimmed()
        ));
        for (i, line) in lines.iter().enumerate() {
            let line_num = format!("{:>4}", i + 1).dimmed();
            box_line(&format!("{} {}", line_num, line.dimmed()));
        }
        box_bottom();
        return true;
    }

    false
}

// ─── Explanation display (Claude Code-inspired) ─────────────────────────────

/// Render a line of markdown-lite (bold, inline code)
pub fn render_markdown(text: &str) -> String {
    let mut result = text.to_string();
    
    // Bold: **text** -> styled bold
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start+2..].find("**") {
            let actual_end = start + 2 + end;
            let content = &result[start+2..actual_end];
            let replacement = content.bold().to_string();
            result.replace_range(start..actual_end+2, &replacement);
        } else {
            break;
        }
    }
    
    // Inline code: `code` -> styled magenta
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start+1..].find('`') {
            let actual_end = start + 1 + end;
            let content = &result[start+1..actual_end];
            let replacement = content.magenta().to_string();
            result.replace_range(start..actual_end+1, &replacement);
        } else {
            break;
        }
    }
    
    result
}

// ─── Explanation display (Claude Code/OpenCode-inspired) ─────────────────────

/// Display a formatted explanation result with a borderless, minimalist aesthetic
pub fn display_explanation(result: &crate::chunker::ExplainResult) {
    eprintln!();

    // Stats line
    let stats = format!(
        "{} lines analyzed{}",
        result.total_lines.to_string().cyan(),
        if result.total_chunks > 1 {
            format!(
                "  •  {} chunks processed",
                result.total_chunks.to_string().cyan()
            )
        } else {
            String::new()
        }
    );
    
    println!("  {}  •  {}", "Explanation".bold().magenta(), stats.dimmed());
    print_rule();
    eprintln!();

    // Overall summary with OpenCode styling
    if result.total_chunks > 1 {
        println!("  {}", "Overview".bold().cyan());
        println!("  {}", "────────".dimmed().cyan());
        for line in result.overall_summary.lines() {
            let line = line.trim();
            if line.is_empty() {
                println!();
                continue;
            }
            if line.starts_with("#") {
                println!("  {}", render_markdown(line.trim_start_matches('#').trim()).bold().magenta());
            } else if line.starts_with("- ") || line.starts_with("* ") {
                println!("  {} {}", "•".cyan(), render_markdown(&line[2..]));
            } else {
                println!("  {}", render_markdown(line));
            }
        }
        println!();

        // Per-chunk details - Borderless
        println!("  {}", "Detailed Analysis".bold().cyan());
        println!("  {}", "─────────────────".dimmed().cyan());
        for chunk in result.chunk_explanations.iter() {
            eprintln!();
            let chunk_title = format!("Lines {}-{}", chunk.start_line, chunk.end_line);
            println!("  {} {}", "󰚗".magenta(), chunk_title.bold());
            
            for line in chunk.explanation.lines() {
                let line = line.trim();
                if line.is_empty() {
                    println!();
                } else if line.starts_with("### ") || line.starts_with("## ") {
                    println!("  {}", render_markdown(line.trim_start_matches('#').trim()).bold().magenta());
                } else if line.starts_with("- ") || line.starts_with("* ") {
                    println!("  {} {}", "•".cyan(), render_markdown(&line[2..]));
                } else {
                    println!("  {}", render_markdown(line));
                }
            }
        }
    } else {
        println!("  {}", "Analysis".bold().cyan());
        println!("  {}", "────────".dimmed().cyan());
        for line in result.overall_summary.lines() {
            let line = line.trim();
            if line.is_empty() {
                println!();
                continue;
            }
            if line.starts_with("## ") || line.starts_with("### ") {
                println!("\n  {}", render_markdown(line.trim_start_matches('#').trim()).bold().magenta());
            } else if line.starts_with("- ") || line.starts_with("* ") {
                println!("  {} {}", "•".cyan(), render_markdown(&line[2..]));
            } else {
                println!("  {}", render_markdown(line));
            }
        }
    }
    println!();

    // Follow-up questions - Borderless
    if !result.follow_up_questions.is_empty() {
        print_rule();
        println!("  {}", "Follow-up Questions".bold().cyan());
        for (i, q) in result.follow_up_questions.iter().enumerate() {
            let cat_end = q.find(']').unwrap_or(0);
            let (cat, text) = if cat_end > 0 {
                (format!("{} ", &q[..=cat_end].cyan()), &q[cat_end+1..])
            } else {
                (String::new(), q.as_str())
            };
            println!("  {} {}{}", format!("{}.", i + 1).dimmed(), cat, render_markdown(text));
        }
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

#[allow(dead_code)]
pub fn prompt_input(prompt: &str) -> io::Result<String> {
    eprint!("{}", prompt);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Get the display length of a string, stripping ANSI escape codes
fn strip_ansi_len(s: &str) -> usize {
    let mut plain = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            plain.push(c);
        }
    }
    UnicodeWidthStr::width(plain.as_str())
}

/// Truncate a string that may contain ANSI escape codes to a maximum display width.
/// Preserves ANSI codes (they don't count towards width) and handles multi-byte UTF-8.
/// Returns the truncated string with an ANSI reset appended if any escape was active.
fn truncate_ansi(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width = 0;
    let mut in_escape = false;
    let mut had_escape = false;

    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            had_escape = true;
            result.push(c);
        } else if in_escape {
            result.push(c);
            if c == 'm' {
                in_escape = false;
            }
        } else {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if width + cw > max_width {
                break;
            }
            result.push(c);
            width += cw;
        }
    }

    // Reset ANSI styling if we had any escape codes
    if had_escape {
        result.push_str("\x1b[0m");
    }

    result
}
