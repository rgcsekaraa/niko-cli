use super::app::{App, Focus, Route};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let input_lines = app.input_buffer.lines().len() as u16;
    let input_height = input_lines.clamp(1, 6) + 2;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, layout[0]);

    let body_chunks = if layout[1].width >= 120 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(layout[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(layout[1])
    };

    draw_output_history(f, app, body_chunks[0]);
    if body_chunks[1].width > 0 {
        draw_sidebar(f, app, body_chunks[1]);
    }

    draw_input_area(f, app, layout[2]);
    draw_footer(f, app, layout[3]);

    if app.show_help {
        draw_help_overlay(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let pulse = [Color::Cyan, Color::LightBlue, Color::Blue, Color::Magenta];
    let c = pulse[(app.spinner_state as usize / 2) % pulse.len()];

    let status = match app.route {
        Route::Processing => "PROCESSING",
        Route::Settings => "SETTINGS",
        Route::Chat => "CHAT",
    };

    let line1 = Line::from(vec![
        Span::styled(
            " NIKO ",
            Style::default()
                .fg(Color::Black)
                .bg(c)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" v{}  ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("{}  ", status),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let line2 = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(&app.status_line, Style::default().fg(Color::Gray)),
    ]);

    f.render_widget(Paragraph::new(Text::from(vec![line1, line2])), area);
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let last_ms = app.last_latency_ms.unwrap_or(0);
    let rag = if app.rag_enabled { "on" } else { "off" };
    let pending = app.pending_command.as_ref().map(|_| "yes").unwrap_or("no");
    let running = if app.command_running { "yes" } else { "no" };
    let pid = app
        .command_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string());
    let planner = if app.planner_steps.is_empty() {
        "none".to_string()
    } else {
        format!("{}/{}", app.planner_cursor, app.planner_steps.len())
    };
    let index_files = app
        .workspace_index
        .as_ref()
        .map(|i| i.indexed_files.to_string())
        .unwrap_or_else(|| "0".to_string());

    let sidebar = vec![
        Line::from(vec![Span::styled(
            "Session",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("messages: {}", app.history.len())),
        Line::from(format!("responses: {}", app.total_responses)),
        Line::from(format!("output chars: {}", app.total_output_chars)),
        Line::from(format!("last latency: {} ms", last_ms)),
        Line::from(format!("rag: {}", rag)),
        Line::from(format!("pending cmd: {}", pending)),
        Line::from(format!("command running: {}", running)),
        Line::from(format!("command pid: {}", pid)),
        Line::from(format!("index files: {}", index_files)),
        Line::from(format!("plan progress: {}", planner)),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("Tab switch focus"),
        Line::from("PgUp/PgDn scroll"),
        Line::from("Home/End jump"),
        Line::from("Ctrl+L clear"),
        Line::from("F1/? help"),
    ];

    f.render_widget(
        Paragraph::new(sidebar)
            .block(
                Block::default()
                    .title("Panel")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mode = if app.focus == Focus::Input {
        Span::styled(
            " INSERT ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " SCROLL ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
    };

    let hints = " /help /search /open /plan /next /run /approve /stop /index /rag on|off ";
    let footer = Line::from(vec![
        mode,
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(footer).alignment(Alignment::Left), area);
}

fn draw_output_history(f: &mut Frame, app: &App, area: Rect) {
    let mut history_text = Text::default();

    if app.history.is_empty()
        && app.result_buffer.is_empty()
        && app.streaming_buffer.is_empty()
        && !app.is_loading
    {
        history_text.lines.push(Line::from(vec![Span::styled(
            " Ready. Ask anything, attach files with @path, or run /help.",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    for entry in &app.history {
        if entry.is_user {
            history_text.lines.push(Line::from(vec![
                Span::styled(" You ", Style::default().fg(Color::Black).bg(Color::Cyan)),
                Span::styled(" ", Style::default()),
            ]));
            for line in entry.text.lines() {
                history_text.lines.push(Line::from(line.to_string()));
            }
        } else {
            history_text.lines.push(Line::from(vec![Span::styled(
                " Niko ",
                Style::default().fg(Color::Black).bg(Color::Green),
            )]));
            let mut in_code = false;
            for line in entry.text.lines() {
                history_text
                    .lines
                    .push(parse_markdown_line(line, &mut in_code));
            }
        }
        history_text.lines.push(Line::from(""));
    }

    if app.is_loading {
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_char = dots[(app.spinner_state as usize / 2) % dots.len()];

        if !app.streaming_buffer.is_empty() {
            let mut in_code = false;
            for line in app.streaming_buffer.lines() {
                history_text
                    .lines
                    .push(parse_markdown_line(line, &mut in_code));
            }
        }
        history_text.lines.push(Line::from(vec![Span::styled(
            format!(" {} thinking...", spinner_char),
            Style::default().fg(Color::Yellow),
        )]));
    }

    let mut total_visual_lines = 0;
    for line in &history_text.lines {
        let w = line.width() as u16;
        total_visual_lines += 1 + w.saturating_sub(1) / area.width.max(1);
    }
    let max_scroll = total_visual_lines.saturating_sub(area.height.saturating_sub(2));

    let current_scroll = if app.is_loading {
        max_scroll
    } else {
        app.result_scroll.min(max_scroll)
    };

    f.render_widget(
        Paragraph::new(history_text)
            .wrap(Wrap { trim: false })
            .scroll((current_scroll, 0))
            .block(
                Block::default()
                    .title("Conversation")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
        area,
    );
}

fn parse_markdown_line<'a>(line: &'a str, in_code_block: &mut bool) -> Line<'a> {
    let line_trim = line.trim();

    if line_trim.starts_with("```") {
        *in_code_block = !*in_code_block;
        if *in_code_block {
            return Line::from(vec![Span::styled(
                " ┌ code",
                Style::default().fg(Color::DarkGray),
            )]);
        }
        return Line::from(vec![Span::styled(
            " └",
            Style::default().fg(Color::DarkGray),
        )]);
    }

    if *in_code_block {
        return Line::from(vec![
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(line, Style::default().fg(Color::Cyan)),
        ]);
    }

    let mut spans = Vec::new();
    let mut current_pos = 0;

    if line_trim.starts_with("#") {
        let text = line_trim.trim_start_matches('#').trim();
        spans.push(Span::styled(
            format!(" {} ", text.to_uppercase()),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
        return Line::from(spans);
    }

    if line_trim.starts_with("- ") || line_trim.starts_with("* ") {
        spans.push(Span::styled(" • ", Style::default().fg(Color::Yellow)));
        current_pos = line.find(|c| c == '-' || c == '*').unwrap_or(0) + 2;
    }

    let remaining = &line[current_pos..];
    let chars: Vec<char> = remaining.chars().collect();
    let mut i = 0;
    let mut text_start = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if i > text_start {
                spans.push(Span::from(chars[text_start..i].iter().collect::<String>()));
            }
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '*') {
                j += 1;
            }
            if j + 1 < chars.len() {
                spans.push(Span::styled(
                    chars[i + 2..j].iter().collect::<String>(),
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::White),
                ));
                i = j + 2;
                text_start = i;
            } else {
                i += 2;
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
                    chars[i + 1..j].iter().collect::<String>(),
                    Style::default().fg(Color::Green),
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
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    app.input_buffer.set_block(
        Block::default()
            .title("Prompt")
            .borders(Borders::ALL)
            .border_style(if app.focus == Focus::Input {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
    );
    app.input_buffer.set_style(focus_style);

    f.render_widget(&app.input_buffer, area);
}

fn draw_help_overlay(f: &mut Frame, app: &App) {
    let popup = centered_rect(70, 70, f.area());

    let text = Text::from(vec![
        Line::from(Span::styled(
            "Niko Commands",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("/help            Toggle this help"),
        Line::from("/providers       List configured providers"),
        Line::from("/provider <name> Switch active provider"),
        Line::from("/models [name]   List provider models"),
        Line::from("/model <id>      Set active model"),
        Line::from("/index           Build/rebuild workspace index"),
        Line::from("/search <q>      Search files in index"),
        Line::from("/open <path>     Preview file in chat"),
        Line::from("/plan <task>     Build task plan"),
        Line::from("/next            Show next planned step"),
        Line::from("/rag on|off      Enable or disable retrieval"),
        Line::from("/run <cmd>       Stage shell command"),
        Line::from("/approve         Execute staged command"),
        Line::from("/stop            Stop running command"),
        Line::from("/deny            Cancel staged command"),
        Line::from("/stats           Session metrics"),
        Line::from("/clear           Clear conversation"),
        Line::from(""),
        Line::from("Tip: use @path/to/file in prompts to attach files."),
        Line::from("Esc closes this panel."),
        Line::from(format!(
            "RAG currently: {}",
            if app.rag_enabled { "on" } else { "off" }
        )),
    ]);

    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .title("Help")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::LightBlue)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
