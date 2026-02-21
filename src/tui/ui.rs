use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use super::app::{App, Focus, Route};

/// Main draw function, dispatches to specific views
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Content / Output History
            Constraint::Length(1), // Divider
            Constraint::Length(3), // Input Area (more compact)
            Constraint::Length(1), // Footer (Keybinds)
        ])
        .split(f.area());

    match app.route {
        Route::Menu => {
            draw_header(f, app, chunks[0]);
            draw_menu(f, app, chunks[1]);
            draw_footer(f, app, chunks[4]);
        }
        Route::Main | Route::Settings | Route::Processing => {
            draw_header(f, app, chunks[0]);
            draw_output_history(f, app, chunks[1]);
            draw_divider(f, chunks[2]);
            draw_input_area(f, app, chunks[3]);
            draw_footer(f, app, chunks[4]);
        }
    }
}

fn draw_header(f: &mut Frame, _app: &App, area: Rect) {
    let header = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled("NIKO", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(" v", Style::default().fg(Color::DarkGray)),
        Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(Color::Cyan)),
        Span::styled(" — Minimalist AI Agent", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(header), area);
}

fn draw_divider(f: &mut Frame, area: Rect) {
    let repeat = (area.width as usize).saturating_div(3);
    let rule = Paragraph::new("───".repeat(repeat)).dim();
    f.render_widget(rule, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mode_text = if app.focus == Focus::Input {
        Span::styled(" INSERT ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" SCROLL ", Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD))
    };

    let binds = match app.route {
        Route::Menu => " 1:Cmd 2:Explain 3:Settings q:Quit ",
        Route::Main => " Tab:Switch Esc:Menu Enter:Submit PgUp/Dn:Scroll ",
        Route::Processing => " Processing... Please wait ",
        Route::Settings => " Enter:Submit Esc:Menu ",
    };

    let footer = Line::from(vec![
        mode_text,
        Span::styled(binds, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(footer).alignment(Alignment::Right), area);
}

fn draw_menu(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Logo/Title spanning
            Constraint::Min(0),
        ])
        .split(area);

    let title = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" Niko v", Style::default().fg(Color::DarkGray)),
            Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(Color::Cyan)),
            Span::styled(" — Minimalist AI Assistant", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
    ]);
    f.render_widget(title, chunks[0]);

    let items = [
        ListItem::new("  1. Generate Command"),
        ListItem::new("  2. Explain Code"),
        ListItem::new("  3. Settings"),
        ListItem::new("  q. Quit"),
    ];

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))
        .highlight_symbol(" ❯ ");

    f.render_stateful_widget(list, chunks[1], &mut app.menu_state);
}

fn draw_output_history(f: &mut Frame, app: &App, area: Rect) {
    let mut history_text = Text::default();
    
    // Welcome message if history is empty
    if app.history.is_empty() && app.result_buffer.is_empty() && app.streaming_buffer.is_empty() && !app.is_loading {
        let welcome = match app.route {
            Route::Main => " Ready to assist. Type a command or paste code.",
            Route::Settings => " Enter configuration changes for your provider.",
            _ => " Awaiting input...",
        };
        history_text.lines.push(Line::from(vec![
            Span::styled(welcome, Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Render stored history
    for entry in &app.history {
        if entry.is_user {
            history_text.lines.push(Line::from(vec![
                Span::styled(" ❯ ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(&entry.text, Style::default().fg(Color::White)),
            ]));
        } else {
            for line in entry.text.lines() {
                history_text.lines.push(line_to_spans(line, true));
            }
        }
        history_text.lines.push(Line::from(""));
    }

    // Render current result buffer (last response)
    if !app.result_buffer.is_empty() {
        for line in app.result_buffer.lines() {
            history_text.lines.push(line_to_spans(line, true));
        }
    }

    // Render streaming buffer if loading
    if app.is_loading {
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_char = dots[(app.spinner_state / 2 % 10) as usize];
        
        if !app.streaming_buffer.is_empty() {
            for line in app.streaming_buffer.lines() {
                history_text.lines.push(line_to_spans(line, true));
            }
        }
        history_text.lines.push(Line::from(vec![
            Span::styled(format!("  {} Processing...", spinner_char), Style::default().fg(Color::Magenta)),
        ]));
    }

    let p = Paragraph::new(history_text)
        .wrap(Wrap { trim: false })
        .scroll((app.result_scroll, 0));
        
    f.render_widget(p, area);
}

/// Convert a markdown-lite line to styled spans
fn line_to_spans(line: &str, _is_agent: bool) -> Line<'_> {
    let mut spans = Vec::new();
    let mut current_pos = 0;
    let line_trim = line.trim();

    // Check for headers
    if line_trim.starts_with("#") {
        let _level = line_trim.chars().take_while(|&c| c == '#').count();
        let text = line_trim.trim_start_matches('#').trim();
        spans.push(Span::styled(
            format!(" {} ", text.to_uppercase()),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        ));
        return Line::from(spans);
    }

    // Bullet points
    if line_trim.starts_with("- ") || line_trim.starts_with("* ") {
        spans.push(Span::styled(" • ", Style::default().fg(Color::Cyan)));
        current_pos = line.find(|c| c == '-' || c == '*').unwrap() + 2;
    } else {
        spans.push(Span::from("  "));
    }

    let remaining = &line[current_pos..];
    
    // Simple bold/code parsing
    let mut i = 0;
    let chars: Vec<char> = remaining.chars().collect();
    let mut text_start = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i+1] == '*' {
            // Flush normal text
            if i > text_start {
                spans.push(Span::from(chars[text_start..i].iter().collect::<String>()));
            }
            // Find end of bold
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '*' && chars[j+1] == '*') {
                j += 1;
            }
            if j + 1 < chars.len() {
                spans.push(Span::styled(
                    chars[i+2..j].iter().collect::<String>(),
                    Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
                ));
                i = j + 2;
                text_start = i;
            } else {
                i += 2;
                // Treat unmatched ** as literal text by not updating text_start
                // but we need to move text_start forward if we want to avoid duplication
                // Actually, if we skip it here, the final push will catch it correctly.
                // But wait, if we find ANOTHER tag later? 
                // Let's just update text_start only on successful match.
            }
        } else if chars[i] == '`' {
            if i > text_start {
                spans.push(Span::from(chars[text_start..i].iter().collect::<String>()));
            }
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            if j < chars.len() {
                spans.push(Span::styled(
                    chars[i+1..j].iter().collect::<String>(),
                    Style::default().fg(Color::Magenta)
                ));
                i = j + 1;
                text_start = i;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    if text_start < chars.len() {
        spans.push(Span::from(chars[text_start..].iter().collect::<String>()));
    }

    Line::from(spans)
}

fn draw_input_area(f: &mut Frame, app: &mut App, area: Rect) {
    let focus_style = if app.focus == Focus::Input {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    app.input_buffer.set_block(
        Block::default()
            .borders(Borders::NONE)
    );
    app.input_buffer.set_style(focus_style);
    
    // Add a small prompt icon
    let input_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    
    f.render_widget(Paragraph::new(" ❯ ").fg(Color::Cyan), input_chunks[0]);
    f.render_widget(&app.input_buffer, input_chunks[1]);
}
