pub mod app;
pub mod events;
pub mod ui;
// pub mod theme; // Not implemented yet

use std::error::Error;
use std::io;
use std::thread;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{chunker, llm, modes};
use app::{App, Route, TuiMessage};
use events::{Event, EventHandler};

pub fn run() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
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
                        KeyCode::Char('1') => app.set_route(Route::CmdInput),
                        KeyCode::Char('2') => app.set_route(Route::ExplainInput),
                        KeyCode::Char('3') => app.set_route(Route::Settings),
                        _ => {}
                    },
                    Route::CmdInput => match key.code {
                        KeyCode::Esc => app.set_route(Route::Menu),
                        KeyCode::Enter => {
                            let query = app.input_buffer.lines().join("\n");
                            if !query.trim().is_empty() {
                                app.is_loading = true;
                                app.set_route(Route::Processing);
                                app.streaming_buffer.clear(); // Clear stream buffer

                                let sender = events.sender.clone();
                                thread::spawn(move || {
                                    // Generate command (non-streaming for now, or we could stream "thinking" logs)
                                    // cmd::generate_command is blocking
                                    let result = modes::cmd::generate_command(&query, None, false);
                                    let msg = match result {
                                        Ok(s) => TuiMessage::CmdResult(Ok(s)),
                                        Err(e) => TuiMessage::CmdResult(Err(e.to_string())),
                                    };
                                    let _ = sender.send(Event::AppMessage(msg));
                                });
                            }
                        }
                        _ => {
                            app.input_buffer.input(key);
                        }
                    },
                    Route::ExplainInput => match key.code {
                        KeyCode::Esc => app.set_route(Route::Menu),
                        KeyCode::Char('d')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            // Submit on Ctrl+D
                            submit_explain(&mut app, &events.sender);
                        }
                        KeyCode::Enter => {
                            // Single enter adds newline. Maybe strict "Ctrl+D" or "Alt+Enter" to submit?
                            // Or heuristic: if ends with two newlines?
                            // For Tui-textarea, Enter just adds newline.
                            // Let's stick to Ctrl+D as primary submit for multiline paste.
                            app.input_buffer.input(key);
                        }
                        _ => {
                            app.input_buffer.input(key);
                        }
                    },
                    Route::Processing => {
                        // While processing, maybe allow Esc to cancel (if we had CancellationToken)
                        // For now, block esc or treat as "background"
                        if key.code == KeyCode::Esc {
                            // Just go back to menu, thread continues but result ignored basically
                            app.is_loading = false;
                            app.set_route(Route::Menu);
                        }
                    }
                    Route::ResultView => match key.code {
                        KeyCode::Esc => app.set_route(Route::Menu),
                        KeyCode::Char('q') => app.exit = true,
                        // TODO: Implement 'c' for copy
                        _ => {}
                    },
                    Route::Settings => match key.code {
                        KeyCode::Esc => app.set_route(Route::Menu),
                        _ => {}
                    },
                }
            }
            Event::Tick => {
                app.on_tick();
            }
            Event::Resize(_, _) => {
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
                                app.set_route(Route::ResultView);
                            }
                            Err(e) => {
                                app.result_buffer = format!("Error: {}", e);
                                app.set_route(Route::ResultView);
                            }
                        }
                    }
                    TuiMessage::ExplainResult(res) => {
                        app.is_loading = false;
                        match res {
                            Ok(explanation) => {
                                // Synthesis is done.
                                app.result_buffer = format!(
                                    "## Summary\n\n{}\n\n## Follow-up Questions\n\n- {}",
                                    explanation.overall_summary,
                                    explanation.follow_up_questions.join("\n- ")
                                );
                                app.set_route(Route::ResultView);
                            }
                            Err(e) => {
                                app.result_buffer = format!("Error: {}", e);
                                app.set_route(Route::ResultView);
                            }
                        }
                    }
                    TuiMessage::Finished => {
                        app.is_loading = false;
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
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn submit_explain(app: &mut App, sender: &std::sync::mpsc::Sender<Event>) {
    let code = app.input_buffer.lines().join("\n");
    if code.trim().is_empty() {
        return;
    }

    app.is_loading = true;
    app.set_route(Route::Processing);
    app.streaming_buffer.clear();

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
