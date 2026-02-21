use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Route};

/// Main draw function, dispatches to specific views
pub fn draw(f: &mut Frame, app: &mut App) {
    match app.route {
        Route::Menu => draw_menu(f, app),
        Route::CmdInput => draw_input(f, app, "Niko Command Generator"),
        Route::ExplainInput => draw_input(f, app, "Niko Code Explainer"),
        Route::Processing => draw_processing(f, app),
        Route::ResultView => draw_result(f, app),
        Route::Settings => draw_settings(f, app),
    }
}

fn draw_menu(f: &mut Frame, _app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    let title = Paragraph::new("Welcome to Niko (v2.0.0)")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" Niko TUI "));
    f.render_widget(title, chunks[0]);

    let items = [
        ListItem::new("1. Generate Command (Cmd Mode)"),
        ListItem::new("2. Explain Code (Explain Mode)"),
        ListItem::new("3. Settings (Configure Providers)"),
        ListItem::new("q. Quit"),
    ];

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Menu "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
    f.render_widget(list, chunks[1]);
}

fn draw_input(f: &mut Frame, app: &mut App, title: &'static str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Input area takes most space
            Constraint::Length(3), // Instructions
        ])
        .split(f.area());

    app.input_buffer.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(&app.input_buffer, chunks[0]);

    let help = Paragraph::new("Press Enter to Submit • Esc for Menu")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[1]);
}

fn draw_processing(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // If we have streaming content, show it!
    if !app.streaming_buffer.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header/Spinner
                Constraint::Min(0),    // Content
            ])
            .split(area);

        // Header with spinner
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (app.spinner_state as usize) % dots.len();
        let spinner_char = dots[idx];
        let header_text = format!(" {} Processing... ", spinner_char);

        let header = Paragraph::new(header_text)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(header, chunks[0]);

        // Content
        let content = Paragraph::new(app.streaming_buffer.clone())
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title(" Live Output "),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(content, chunks[1]);

        return;
    }

    // Default "Thinking" view if no stream yet
    let center_vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Percentage(40),
        ])
        .split(area);

    let center_area = center_vert[1];

    // Manual spinner logic
    let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let idx = (app.spinner_state as usize) % dots.len();
    let spinner_char = dots[idx];

    let text = format!(" {} Thinking... ", spinner_char);

    let p = Paragraph::new(text).alignment(Alignment::Center).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    f.render_widget(p, center_area);
}

fn draw_result(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Result content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    let result = Paragraph::new(app.result_buffer.clone())
        .block(Block::default().borders(Borders::ALL).title(" Result "))
        .wrap(Wrap { trim: true });
    f.render_widget(result, chunks[0]);

    let help = Paragraph::new("Esc: Back • c: Copy • q: Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[1]);
}

fn draw_settings(f: &mut Frame, _f_app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings (TUI) ");
    let text = Paragraph::new("Please run `niko settings configure` in CLI mode for now.")
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(text, f.area());
}
