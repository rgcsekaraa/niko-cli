pub mod app;
pub mod events;
pub mod ui;
// pub mod theme; // Not implemented yet

use std::error::Error;
use std::io;
use std::thread;
use std::time::Duration;

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Style, Terminal};

use crate::{chunker, llm, modes};
use app::{App, Focus, Route, TuiMessage};
use events::{Event, EventHandler};

pub fn run() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();
    let events = EventHandler::new(Duration::from_millis(100)); // 10 ticks/sec

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match events.next()? {
            Event::Key(key) => {
                match app.route {
                    Route::Menu => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.exit = true,
                        KeyCode::Char('1') | KeyCode::Char('2') => app.set_route(Route::Main),
                        KeyCode::Char('3') => app.set_route(Route::Settings),
                        KeyCode::Up => {
                            let i = match app.menu_state.selected() {
                                Some(i) => if i == 0 { 3 } else { i - 1 },
                                None => 0,
                            };
                            app.menu_state.select(Some(i));
                        }
                        KeyCode::Down => {
                            let i = match app.menu_state.selected() {
                                Some(i) => if i >= 3 { 0 } else { i + 1 },
                                None => 0,
                            };
                            app.menu_state.select(Some(i));
                        }
                        KeyCode::Enter => {
                            match app.menu_state.selected() {
                                Some(0) | Some(1) => app.set_route(Route::Main),
                                Some(2) => app.set_route(Route::Settings),
                                Some(3) => app.exit = true,
                                _ => {}
                            }
                        }
                        _ => {}
                    },
                    Route::Main => {
                        if key.code == KeyCode::Esc {
                            app.set_route(Route::Menu);
                        } else if key.code == KeyCode::Tab {
                            app.focus = if app.focus == Focus::Input { Focus::Output } else { Focus::Input };
                        } else if key.code == KeyCode::Enter && app.focus == Focus::Input {
                            let mut input = app.input_buffer.lines().join("\n");
                            
                            // Combine with pasted code if it exists
                            if let Some(pasted) = app.pasted_code.take() {
                                input = format!("{}\n\n{}", input.trim(), pasted);
                            }
                            
                            if !input.trim().is_empty() {
                                // Before starting new request, archive old results to history
                                if !app.result_buffer.is_empty() {
                                    app.history.push(crate::tui::app::HistoryEntry {
                                        is_user: false,
                                        text: app.result_buffer.clone(),
                                    });
                                    app.result_buffer.clear();
                                }
                                
                                // Push user input to history (truncate for display if it's too long)
                                let display_text = if input.lines().count() > 5 {
                                    let first_lines = input.lines().take(3).collect::<Vec<_>>().join("\n");
                                    format!("{}\n... [{} lines omitted]", first_lines, input.lines().count() - 3)
                                } else {
                                    input.clone()
                                };

                                app.history.push(crate::tui::app::HistoryEntry {
                                    is_user: true,
                                    text: display_text,
                                });

                                // Simple slash command detection
                                if input.starts_with("/explain") || input.lines().count() > 1 {
                                    submit_explain(&mut app, &events.sender, input);
                                } else {
                                    let query = if input.starts_with("/cmd ") {
                                        input.trim_start_matches("/cmd ").to_string()
                                    } else {
                                        input.clone()
                                    };
                                    
                                    app.is_loading = true;
                                    app.set_route(Route::Processing);
                                    app.streaming_buffer.clear();

                                    let sender = events.sender.clone();
                                    thread::spawn(move || {
                                        let result = modes::cmd::generate_command(&query, None, false);
                                        let msg = match result {
                                            Ok(s) => TuiMessage::CmdResult(Ok(s)),
                                            Err(e) => TuiMessage::CmdResult(Err(e.to_string())),
                                        };
                                        let _ = sender.send(Event::AppMessage(msg));
                                    });
                                }
                                app.input_buffer = tui_textarea::TextArea::default();
                                app.input_buffer.set_cursor_line_style(Style::default());
                            }
                        } else {
                            if app.focus == Focus::Output {
                                match key.code {
                                    KeyCode::Up => app.result_scroll = app.result_scroll.saturating_sub(1),
                                    KeyCode::Down => app.result_scroll = app.result_scroll.saturating_add(1),
                                    KeyCode::PageUp => app.result_scroll = app.result_scroll.saturating_sub(10),
                                    KeyCode::PageDown => app.result_scroll = app.result_scroll.saturating_add(10),
                                    _ => {}
                                }
                            } else {
                                app.input_buffer.input(key);
                            }
                        }
                    }
                    Route::Processing => {
                        if key.code == KeyCode::Esc {
                            app.is_loading = false;
                            app.set_route(Route::Menu);
                        }
                    }
                    Route::Settings => match key.code {
                        KeyCode::Esc => app.set_route(Route::Menu),
                        _ => {}
                    },
                }
            }
            Event::Paste(s) => {
                if app.route == Route::Main && app.focus == Focus::Input {
                    let lines_count = s.lines().count();
                    if lines_count > 1 {
                        app.pasted_code = Some(s);
                        app.input_buffer.insert_str(&format!("[Pasted {} lines of code — Press Enter to submit]", lines_count));
                    } else {
                        app.input_buffer.insert_str(&s);
                    }
                }
            }
            Event::Tick => {
                app.on_tick();
            }
            Event::Resize => {
                // Ratatui handles resize automatically
            }
            Event::AppMessage(msg) => {
                match msg {
                    TuiMessage::Token(s) => {
                        app.streaming_buffer.push_str(&s);
                        // Also update result buffer live so user sees it?
                        // Or Processing view should render streaming_buffer?
                        // If we are in Processing route, user needs to see the stream.
                    }
                    TuiMessage::CmdResult(res) => {
                        app.is_loading = false;
                        match res {
                            Ok(cmd) => {
                                app.result_buffer = cmd;
                                app.set_route(Route::Main);
                            }
                            Err(e) => {
                                app.result_buffer = format!("Error: {}", e);
                                app.set_route(Route::Main);
                            }
                        }
                    }
                    TuiMessage::ExplainResult(res) => {
                        app.is_loading = false;
                        app.set_route(Route::Main);
                        match res {
                            Ok(explanation) => {
                                app.result_buffer = format!(
                                    "## Summary\n\n{}\n\n## Follow-up Questions\n\n- {}",
                                    explanation.overall_summary,
                                    explanation.follow_up_questions.join("\n- ")
                                );
                            }
                            Err(e) => {
                                app.result_buffer = format!("Error: {}", e);
                            }
                        }
                    }
                }
            }
        }

        if app.exit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn submit_explain(app: &mut App, sender: &std::sync::mpsc::Sender<Event>, code: String) {
    if code.trim().is_empty() {
        return;
    }

    app.is_loading = true;
    app.streaming_buffer.clear();
    // app.result_buffer.clear(); // Already handled by the caller who pushes to history
    use crate::tui::app::Focus;
    app.focus = Focus::Output;

    let sender_token = sender.clone();
    let sender_final = sender.clone();

    thread::spawn(move || {
        // Need to fetch provider first. Since App holds state, but here we spawn thread...
        // We need to read provider from config. `llm::get_provider` does that.
        let provider_res = llm::get_provider(None);

        match provider_res {
            Ok(provider) => {
                // Determine if we can stream
                // We use explain_code with callback
                let res = chunker::explain_code(
                    &code,
                    provider.as_ref(),
                    false,
                    Some(|token: &str| {
                        let _ = sender_token
                            .send(Event::AppMessage(TuiMessage::Token(token.to_string())));
                    }),
                );

                let _ = sender_final.send(Event::AppMessage(match res {
                    Ok(r) => TuiMessage::ExplainResult(Ok(r)),
                    Err(e) => TuiMessage::ExplainResult(Err(e.to_string())),
                }));
            }
            Err(e) => {
                let _ = sender_final.send(Event::AppMessage(TuiMessage::ExplainResult(Err(
                    e.to_string()
                ))));
            }
        }
    });
}
