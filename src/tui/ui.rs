use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Focus, Route};

/// Main draw function, dispatches to specific views
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);

    match app.route {
        Route::Menu => draw_menu(f, app, chunks[1]),
        Route::CmdInput | Route::Settings => draw_single_pane(f, app, chunks[1]),
        Route::Processing | Route::ResultView => {
            // Depending on context (was it cmd or explain?), we might want different layouts.
            // For now, let's just use single pane for Cmd results, and split pane for Explain.
            // Wait, we don't have an AppMode yet. Let's just use single pane for now, unless we can infer it.
            // Actually, if input_buffer has substantial content, it's likely Explain? No, Cmd has input too.
            // Let's use single pane for everything except ExplainInput for now, but wait, we *want* split pane.
            // I'll update it later if needed.
            draw_result_pane(f, app, chunks[1])
        }
        Route::ExplainInput => draw_explain_split(f, app, chunks[1]),
    }

    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mode_name = match app.route {
        Route::Menu => "Main Menu",
        Route::CmdInput => "Cmd Generator",
        Route::ExplainInput => "Code Explainer",
        Route::Processing => "Processing...",
        Route::ResultView => "Results",
        Route::Settings => "Settings",
    };

    let header_text = format!(" Niko v{} — {} ", env!("CARGO_PKG_VERSION"), mode_name);
    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    f.render_widget(header, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let binds = match app.route {
        Route::Menu => " [1] Cmd  [2] Explain  [3] Settings  [q] Quit ",
        Route::ExplainInput => {
            if app.focus == Focus::Left {
                " [Ctrl+D] Submit  [Tab] Switch Pane  [Esc] Menu "
            } else {
                " [Tab] Switch Pane  [Up/Down] Scroll  [Esc] Menu "
            }
        }
        Route::ResultView => " [Up/Down] Scroll  [Esc] Menu  [q] Quit ",
        _ => " [Enter] Submit  [Esc] Menu ",
    };

    let footer = Paragraph::new(binds)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, area);
}

fn draw_menu(f: &mut Frame, _app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(area);

    let items = [
        ListItem::new("1. Generate Command (Cmd Mode)"),
        ListItem::new("2. Explain Code (Explain Mode)"),
        ListItem::new("3. Settings (Configure Providers)"),
        ListItem::new("q. Quit"),
    ];

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Select an option "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    f.render_widget(list, chunks[1]);
}

fn draw_single_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.route {
        Route::CmdInput => " Describe Command ",
        Route::Settings => " Settings ",
        _ => " Input ",
    };

    app.input_buffer.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title)
            .title_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(&app.input_buffer, area);
}

fn draw_result_pane(f: &mut Frame, app: &App, area: Rect) {
    if app.is_loading {
        // Spinner
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_char = dots[(app.spinner_state as usize) % dots.len()];
        let text = format!("\n\n {} Processing... \n\n{}", spinner_char, app.streaming_buffer);

        let p = Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    } else {
        let p = Paragraph::new(app.result_buffer.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Output "),
            )
            .wrap(Wrap { trim: true })
            .scroll((app.result_scroll, 0));
        f.render_widget(p, area);
    }
}

fn draw_explain_split(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left Pane: Code Editor
    let left_style = if app.focus == Focus::Left {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    app.input_buffer.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Code to Explain ")
            .border_style(left_style)
            .title_style(left_style),
    );
    f.render_widget(&app.input_buffer, chunks[0]);

    // Right Pane: Output stream or Results
    let right_style = if app.focus == Focus::Right {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let right_title = if app.is_loading {
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_char = dots[(app.spinner_state as usize) % dots.len()];
        format!(" {} Explaining... ", spinner_char)
    } else if !app.result_buffer.is_empty() {
        " Explanation Complete ".to_string()
    } else {
        " Output ".to_string()
    };

    let right_content = if !app.result_buffer.is_empty() {
        app.result_buffer.clone()
    } else if !app.streaming_buffer.is_empty() {
        app.streaming_buffer.clone()
    } else {
        "Awaiting input...".to_string()
    };

    let right_pane = Paragraph::new(right_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(right_title)
                .border_style(right_style)
                .title_style(right_style),
        )
        .wrap(Wrap { trim: true })
        .scroll((app.streaming_scroll, 0)); // Using streaming_scroll for both

    f.render_widget(right_pane, chunks[1]);
}
