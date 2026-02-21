pub mod actions;
pub mod app;
pub mod events;
pub mod ui;
pub mod workspace;

use std::error::Error;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyCode, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Style, Terminal};

use crate::llm;
use actions::{handle_slash_command, prepare_user_input, spawn_background_indexer, spawn_warmup};
use app::{App, Focus, HistoryEntry, Route, TuiMessage};
use events::{Event, EventHandler};

pub fn run() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let events = EventHandler::new(Duration::from_millis(100));
    let sender = events.sender.clone();
    let base_system_prompt = crate::prompt::chat_system_prompt(&crate::prompt::gather_context());

    spawn_warmup(sender.clone());
    spawn_background_indexer(sender.clone());

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match events.next()? {
            Event::Key(key) => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                {
                    app.exit = true;
                    continue;
                }

                if key.code == KeyCode::F(1)
                    || (key.code == KeyCode::Char('?') && app.focus == Focus::Input)
                {
                    app.show_help = !app.show_help;
                    continue;
                }

                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
                    app.history.clear();
                    app.result_scroll = 0;
                    app.status_line = "Cleared history".to_string();
                    continue;
                }

                if app.show_help && key.code == KeyCode::Esc {
                    app.show_help = false;
                    continue;
                }

                let now = Instant::now();
                let is_paste = now.duration_since(app.last_key_time) < Duration::from_millis(5);
                app.last_key_time = now;

                match app.route {
                    Route::Chat => {
                        if key.code == KeyCode::Esc {
                            app.set_route(Route::Settings);
                        } else if key.code == KeyCode::Tab {
                            if is_paste && app.focus == Focus::Input {
                                app.input_buffer.insert_str("    ");
                                continue;
                            }
                            app.focus = if app.focus == Focus::Input {
                                Focus::Output
                            } else {
                                Focus::Input
                            };
                        } else if key.code == KeyCode::Enter && app.focus == Focus::Input {
                            if is_paste {
                                app.input_buffer.insert_newline();
                                continue;
                            }

                            let mut input = app.input_buffer.lines().join("\n");
                            if let Some(pasted) = app.pasted_code.take() {
                                input = format!("{}\n\n{}", input.trim(), pasted);
                            }

                            if !input.trim().is_empty() {
                                if handle_slash_command(&mut app, input.trim(), &sender) {
                                    app.input_buffer = tui_textarea::TextArea::default();
                                    app.input_buffer.set_cursor_line_style(Style::default());
                                    continue;
                                }

                                let final_input = prepare_user_input(&mut app, &input);

                                app.history.push(HistoryEntry {
                                    is_user: true,
                                    text: final_input.replace('\r', ""),
                                });

                                let mut messages = Vec::new();
                                messages.push(crate::llm::Message {
                                    role: crate::llm::Role::System,
                                    content: base_system_prompt.clone(),
                                });

                                for entry in &app.history {
                                    messages.push(crate::llm::Message {
                                        role: if entry.is_user {
                                            crate::llm::Role::User
                                        } else {
                                            crate::llm::Role::Assistant
                                        },
                                        content: entry.text.clone(),
                                    });
                                }

                                app.is_loading = true;
                                app.status_line = "Running model inference...".to_string();
                                app.set_route(Route::Processing);
                                app.streaming_buffer.clear();
                                app.result_buffer.clear();
                                app.focus = Focus::Output;

                                let sender_token = sender.clone();
                                let sender_final = sender.clone();

                                thread::spawn(move || {
                                    let started = Instant::now();
                                    let provider_res = llm::get_provider(None);
                                    match provider_res {
                                        Ok(provider) => {
                                            let res = llm::generate_streaming(
                                                provider.as_ref(),
                                                &messages,
                                                2048,
                                                &mut |token: &str| {
                                                    let _ = sender_token.send(Event::AppMessage(
                                                        TuiMessage::Token(token.to_string()),
                                                    ));
                                                },
                                            );

                                            match res {
                                                Ok(final_text) => {
                                                    let _ = sender_final.send(Event::AppMessage(
                                                        TuiMessage::StreamFinished {
                                                            latency_ms: started
                                                                .elapsed()
                                                                .as_millis(),
                                                            output_chars: final_text.len(),
                                                        },
                                                    ));
                                                }
                                                Err(e) => {
                                                    let _ = sender_final.send(Event::AppMessage(
                                                        TuiMessage::Error(e.to_string()),
                                                    ));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = sender_final.send(Event::AppMessage(
                                                TuiMessage::Error(e.to_string()),
                                            ));
                                        }
                                    }
                                });

                                app.input_buffer = tui_textarea::TextArea::default();
                                app.input_buffer.set_cursor_line_style(Style::default());
                            }
                        } else if app.focus == Focus::Output {
                            match key.code {
                                KeyCode::Up => {
                                    app.result_scroll = app.result_scroll.saturating_sub(1)
                                }
                                KeyCode::Down => {
                                    app.result_scroll = app.result_scroll.saturating_add(1)
                                }
                                KeyCode::PageUp => {
                                    app.result_scroll = app.result_scroll.saturating_sub(10)
                                }
                                KeyCode::PageDown => {
                                    app.result_scroll = app.result_scroll.saturating_add(10)
                                }
                                KeyCode::Home => app.result_scroll = 0,
                                KeyCode::End => app.result_scroll = u16::MAX,
                                _ => {}
                            }
                        } else {
                            app.input_buffer.input(key);
                        }
                    }
                    Route::Processing => {
                        if key.code == KeyCode::Esc {
                            app.is_loading = false;
                            app.status_line = "Cancelled current request".to_string();
                            app.set_route(Route::Chat);
                        }
                    }
                    Route::Settings => {
                        if key.code == KeyCode::Esc {
                            app.set_route(Route::Chat);
                        }
                    }
                }
            }
            Event::Paste(s) => {
                if app.route == Route::Chat && app.focus == Focus::Input {
                    let s_clean = s.replace('\r', "");
                    let lines_count = s_clean.lines().count();
                    if lines_count > 1 {
                        app.pasted_code = Some(s_clean);
                        app.input_buffer.insert_str(&format!(
                            "[Pasted {} lines - press Enter to submit]",
                            lines_count
                        ));
                    } else {
                        app.input_buffer.insert_str(&s_clean);
                    }
                }
            }
            Event::Tick => app.on_tick(),
            Event::Resize => {}
            Event::AppMessage(msg) => match msg {
                TuiMessage::Token(s) => {
                    let clean = s.replace('\r', "").replace('\t', "    ");
                    app.streaming_buffer.push_str(&clean);
                }
                TuiMessage::StreamFinished {
                    latency_ms,
                    output_chars,
                } => {
                    let final_response = app.streaming_buffer.clone();
                    app.is_loading = false;
                    app.set_route(Route::Chat);

                    if !final_response.trim().is_empty() {
                        app.history.push(HistoryEntry {
                            is_user: false,
                            text: final_response,
                        });
                    }
                    app.total_responses += 1;
                    app.total_output_chars += output_chars as u64;
                    app.last_latency_ms = Some(latency_ms);
                    app.status_line = format!(
                        "Done in {} ms | {} chars | responses {}",
                        latency_ms, output_chars, app.total_responses
                    );
                    app.streaming_buffer.clear();
                    app.focus = Focus::Input;
                }
                TuiMessage::Error(e) => {
                    let partial = app.streaming_buffer.trim().to_string();
                    app.is_loading = false;
                    app.set_route(Route::Chat);
                    app.streaming_buffer.clear();
                    let text = if partial.is_empty() {
                        format!("**Error:** {}", e)
                    } else {
                        format!("{}\n\n**Error:** {}", partial, e)
                    };
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text,
                    });
                    app.status_line = "Request failed".to_string();
                }
                TuiMessage::WarmupStatus(status) => {
                    app.status_line = status;
                }
                TuiMessage::WorkspaceIndexReady { index, source } => {
                    app.workspace_index = Some(index);
                    app.status_line = format!("Index synced ({})", source);
                }
                TuiMessage::CommandStarted { pid, cmd } => {
                    app.command_pid = Some(pid);
                    app.command_running = true;
                    app.status_line = format!("Running pid {}: {}", pid, cmd);
                }
                TuiMessage::CommandStream(chunk) => {
                    app.streaming_buffer.push_str(&chunk);
                    app.is_loading = true;
                }
                TuiMessage::CommandOutput { cmd, output } => {
                    app.is_loading = false;
                    app.command_running = false;
                    app.command_pid = None;
                    app.streaming_buffer.clear();
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: format!("```bash\n$ {}\n{}\n```", cmd, output),
                    });
                    app.status_line = format!("Command completed: {}", cmd);
                }
            },
        }

        if app.exit {
            break;
        }
    }

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
