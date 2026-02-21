use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::llm;
use crate::tui::app::{App, HistoryEntry, TuiMessage};
use crate::tui::events::Event;
use crate::tui::workspace;

pub fn spawn_warmup(sender: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let started = Instant::now();
        let status = match llm::get_provider(None) {
            Ok(provider) => {
                let ready = if provider.is_available() {
                    "ready"
                } else {
                    "configured (connectivity pending)"
                };
                format!(
                    "Provider {} {} in {} ms",
                    provider.name(),
                    ready,
                    started.elapsed().as_millis()
                )
            }
            Err(e) => format!("Provider warmup failed: {}", e),
        };
        let _ = sender.send(Event::AppMessage(TuiMessage::WarmupStatus(status)));
    });
}

pub fn spawn_background_indexer(sender: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cache_path = crate::config::config_dir().join("workspace_index.json");

        loop {
            let index =
                workspace::WorkspaceIndex::build_incremental(&cwd, &cache_path, 1600, 256 * 1024);
            let source = if cache_path.exists() {
                "incremental"
            } else {
                "fresh"
            }
            .to_string();
            let _ = sender.send(Event::AppMessage(TuiMessage::WorkspaceIndexReady {
                index,
                source,
            }));
            thread::sleep(Duration::from_secs(45));
        }
    });
}

pub fn prepare_user_input(app: &mut App, input: &str) -> String {
    let mut final_input = enrich_with_attached_files(input);
    if app.rag_enabled {
        final_input = enrich_with_workspace_context(app, &final_input);
    }
    final_input
}

pub fn handle_slash_command(app: &mut App, input: &str, sender: &mpsc::Sender<Event>) -> bool {
    if !input.starts_with('/') {
        return false;
    }

    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or_default();

    match cmd {
        "/help" => {
            app.show_help = true;
            true
        }
        "/clear" => {
            app.history.clear();
            app.streaming_buffer.clear();
            app.result_buffer.clear();
            app.result_scroll = 0;
            app.status_line = "Cleared history".to_string();
            true
        }
        "/providers" => {
            match crate::config::load() {
                Ok(cfg) => {
                    let mut lines =
                        vec![format!("Active: `{}`", cfg.active_provider), String::new()];
                    lines.push("Configured providers:".to_string());
                    for (name, p) in cfg.providers {
                        let mark = if name == cfg.active_provider {
                            "*"
                        } else {
                            " "
                        };
                        let model = if p.model.is_empty() {
                            "(no model)".to_string()
                        } else {
                            p.model
                        };
                        lines.push(format!("- {} `{}` ({}) - {}", mark, name, p.kind, model));
                    }
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: lines.join("\n"),
                    });
                }
                Err(e) => push_error(app, &e.to_string()),
            }
            true
        }
        "/provider" => {
            let name = parts.next().unwrap_or_default();
            if name.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/provider <name>`".to_string(),
                });
                return true;
            }
            match crate::config::set_active_provider(name) {
                Ok(()) => {
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: format!("Active provider set to `{}`", name),
                    });
                    app.status_line = format!("Switched provider to {}", name);
                }
                Err(e) => push_error(app, &e.to_string()),
            }
            true
        }
        "/models" => {
            let override_provider = parts.next().map(|s| s.to_string());
            let list = (|| -> anyhow::Result<(String, Vec<crate::llm::ModelInfo>)> {
                let cfg = crate::config::load()?;
                let provider_name =
                    override_provider.unwrap_or_else(|| cfg.active_provider.clone());
                let pcfg = cfg.providers.get(&provider_name).ok_or_else(|| {
                    anyhow::anyhow!("Provider '{}' is not configured.", provider_name)
                })?;
                let provider = crate::llm::from_config(&provider_name, pcfg)?;
                Ok((provider_name, provider.list_models()?))
            })();

            match list {
                Ok((provider_name, models)) => {
                    let mut lines = vec![format!("Models for `{}`:", provider_name)];
                    if models.is_empty() {
                        lines.push("- (none returned)".to_string());
                    } else {
                        for model in models.into_iter().take(40) {
                            lines.push(format!("- `{}`", model.id));
                        }
                    }
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: lines.join("\n"),
                    });
                }
                Err(e) => push_error(app, &e.to_string()),
            }
            true
        }
        "/model" => {
            let model = parts.next().unwrap_or_default();
            if model.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/model <id>`".to_string(),
                });
                return true;
            }
            match crate::config::active_provider() {
                Ok((name, _)) => match crate::config::set_provider_field(&name, "model", model) {
                    Ok(()) => {
                        app.history.push(HistoryEntry {
                            is_user: false,
                            text: format!("Model for `{}` set to `{}`", name, model),
                        });
                        app.status_line = format!("Model updated: {}", model);
                    }
                    Err(e) => push_error(app, &e.to_string()),
                },
                Err(e) => push_error(app, &e.to_string()),
            }
            true
        }
        "/stats" => {
            match crate::config::load() {
                Ok(cfg) => {
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: format!(
                            "Session stats:\n- Messages: {}\n- Responses: {}\n- Total output chars: {}\n- Last latency: {} ms\n- Active provider: `{}`\n- RAG: {}",
                            app.history.len(),
                            app.total_responses,
                            app.total_output_chars,
                            app.last_latency_ms.unwrap_or(0),
                            cfg.active_provider,
                            if app.rag_enabled { "on" } else { "off" }
                        ),
                    });
                }
                Err(e) => push_error(app, &e.to_string()),
            }
            true
        }
        "/index" => {
            build_workspace_index(app, true);
            true
        }
        "/search" => {
            let query = input.strip_prefix("/search").unwrap_or_default().trim();
            if query.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/search <query>`".to_string(),
                });
                return true;
            }
            if app.workspace_index.is_none() {
                build_workspace_index(app, false);
            }
            if let Some(index) = app.workspace_index.as_ref() {
                let paths = index.search_paths(query, 20);
                let text = if paths.is_empty() {
                    format!("No matching files for `{}`.", query)
                } else {
                    let mut out = vec![format!("Matches for `{}`:", query)];
                    for p in paths {
                        out.push(format!("- `{}`", p));
                    }
                    out.join("\n")
                };
                app.history.push(HistoryEntry {
                    is_user: false,
                    text,
                });
            }
            true
        }
        "/open" => {
            let path = input.strip_prefix("/open").unwrap_or_default().trim();
            if path.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/open <path>`".to_string(),
                });
                return true;
            }
            let payload = enrich_with_attached_files(&format!("@{}", path));
            if payload.contains("[Attached file:") {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: payload,
                });
            } else {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: format!("Could not open `{}` as UTF-8 text file.", path),
                });
            }
            true
        }
        "/plan" => {
            let task = input.strip_prefix("/plan").unwrap_or_default().trim();
            if task.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/plan <task>`".to_string(),
                });
                return true;
            }
            app.planner_steps = build_local_plan(task);
            app.planner_cursor = 0;
            let mut out = vec!["Plan created:".to_string()];
            for (i, step) in app.planner_steps.iter().enumerate() {
                out.push(format!("{}. {}", i + 1, step));
            }
            out.push("Use `/next` to view next step.".to_string());
            app.history.push(HistoryEntry {
                is_user: false,
                text: out.join("\n"),
            });
            true
        }
        "/next" => {
            if app.planner_steps.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "No active plan. Use `/plan <task>`.".to_string(),
                });
                return true;
            }
            if app.planner_cursor >= app.planner_steps.len() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Plan complete.".to_string(),
                });
                return true;
            }
            let step = app.planner_steps[app.planner_cursor].clone();
            app.planner_cursor += 1;
            app.history.push(HistoryEntry {
                is_user: false,
                text: format!("Step {}: {}", app.planner_cursor, step),
            });
            true
        }
        "/rag" => {
            let arg = parts.next().unwrap_or_default().to_lowercase();
            match arg.as_str() {
                "on" => {
                    app.rag_enabled = true;
                    app.status_line = "RAG enabled".to_string();
                }
                "off" => {
                    app.rag_enabled = false;
                    app.status_line = "RAG disabled".to_string();
                }
                _ => {
                    app.history.push(HistoryEntry {
                        is_user: false,
                        text: "Usage: `/rag on|off`".to_string(),
                    });
                }
            }
            true
        }
        "/run" => {
            let command = input
                .strip_prefix("/run")
                .unwrap_or_default()
                .trim()
                .to_string();
            if command.is_empty() {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Usage: `/run <shell command>`".to_string(),
                });
                return true;
            }

            app.pending_command = Some(command.clone());
            app.history.push(HistoryEntry {
                is_user: false,
                text: format!(
                    "Pending command:\n```bash\n{}\n```\nApprove with `/approve` or cancel with `/deny`.",
                    command
                ),
            });
            true
        }
        "/approve" => {
            let Some(command) = app.pending_command.take() else {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "No pending command. Use `/run <cmd>` first.".to_string(),
                });
                return true;
            };

            if is_blocked_command(&command) {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "Command blocked by safety rules.".to_string(),
                });
                return true;
            }

            app.command_running = true;
            app.is_loading = true;
            run_command_async(command, sender.clone());
            app.status_line = "Running approved command...".to_string();
            true
        }
        "/stop" => {
            if let Some(pid) = app.command_pid {
                match stop_running_command(pid) {
                    Ok(()) => {
                        app.status_line = format!("Sent stop signal to pid {}", pid);
                        app.history.push(HistoryEntry {
                            is_user: false,
                            text: format!("Stop signal sent to process `{}`.", pid),
                        });
                    }
                    Err(e) => push_error(app, &format!("stopping process {}: {}", pid, e)),
                }
            } else {
                app.history.push(HistoryEntry {
                    is_user: false,
                    text: "No running command to stop.".to_string(),
                });
            }
            true
        }
        "/deny" => {
            app.pending_command = None;
            app.status_line = "Pending command discarded".to_string();
            true
        }
        _ => {
            app.history.push(HistoryEntry {
                is_user: false,
                text: format!("Unknown command: `{}`. Use `/help`.", cmd),
            });
            true
        }
    }
}

pub fn run_command_async(cmd: String, sender: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let mut child = if cfg!(target_os = "windows") {
            match Command::new("cmd")
                .args(["/C", &cmd])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = sender.send(Event::AppMessage(TuiMessage::CommandOutput {
                        cmd,
                        output: format!("Failed to run command: {}", e),
                    }));
                    return;
                }
            }
        } else {
            match Command::new("sh")
                .args(["-lc", &cmd])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = sender.send(Event::AppMessage(TuiMessage::CommandOutput {
                        cmd,
                        output: format!("Failed to run command: {}", e),
                    }));
                    return;
                }
            }
        };

        let _ = sender.send(Event::AppMessage(TuiMessage::CommandStarted {
            pid: child.id(),
            cmd: cmd.clone(),
        }));

        let Some(mut out) = child.stdout.take() else {
            let _ = sender.send(Event::AppMessage(TuiMessage::CommandOutput {
                cmd,
                output: "Failed to capture stdout".to_string(),
            }));
            return;
        };
        let Some(mut err) = child.stderr.take() else {
            let _ = sender.send(Event::AppMessage(TuiMessage::CommandOutput {
                cmd,
                output: "Failed to capture stderr".to_string(),
            }));
            return;
        };

        let sender_out = sender.clone();
        let out_thread = thread::spawn(move || -> String {
            let mut buf = [0_u8; 2048];
            let mut acc = String::new();
            loop {
                match out.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                        acc.push_str(&chunk);
                        let _ =
                            sender_out.send(Event::AppMessage(TuiMessage::CommandStream(chunk)));
                    }
                    Err(_) => break,
                }
            }
            acc
        });

        let sender_err = sender.clone();
        let err_thread = thread::spawn(move || -> String {
            let mut buf = [0_u8; 1024];
            let mut acc = String::new();
            loop {
                match err.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                        acc.push_str(&chunk);
                        let _ =
                            sender_err.send(Event::AppMessage(TuiMessage::CommandStream(chunk)));
                    }
                    Err(_) => break,
                }
            }
            acc
        });

        let status = child.wait().ok();
        let stdout_all = out_thread.join().unwrap_or_default();
        let stderr_all = err_thread.join().unwrap_or_default();
        let mut rendered = format!("{}{}", stdout_all, stderr_all);
        if rendered.trim().is_empty() {
            rendered = format!("(no output, exit={:?})", status);
        }
        if rendered.len() > 16_000 {
            let mut end = 16_000;
            while end > 0 && !rendered.is_char_boundary(end) {
                end -= 1;
            }
            rendered.truncate(end);
            rendered.push_str("\n[...truncated]");
        }

        let _ = sender.send(Event::AppMessage(TuiMessage::CommandOutput {
            cmd,
            output: rendered,
        }));
    });
}

fn stop_running_command(pid: u32) -> Result<(), String> {
    if cfg!(target_os = "windows") {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|e| format!("taskkill failed: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("taskkill exited with status: {}", status))
        }
    } else {
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .map_err(|e| format!("kill failed: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("kill exited with status: {}", status))
        }
    }
}

fn is_blocked_command(cmd: &str) -> bool {
    let lowered = cmd.to_lowercase();

    if let Ok(cfg) = crate::config::load() {
        for blocked in cfg.safety.blocked_commands {
            if lowered.contains(&blocked.to_lowercase()) {
                return true;
            }
        }
    }

    let hard_block = ["rm -rf /", "mkfs", "dd if=/dev/zero", "> /dev/sda"];
    hard_block
        .iter()
        .any(|token| lowered.contains(&token.to_lowercase()))
}

fn build_workspace_index(app: &mut App, force_rebuild: bool) {
    if app.workspace_index.is_some() && !force_rebuild {
        return;
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cache_path = crate::config::config_dir().join("workspace_index.json");
    let index = if force_rebuild {
        let fresh = workspace::WorkspaceIndex::build(&cwd, 1600, 256 * 1024);
        let _ = fresh.save_cache(&cache_path);
        fresh
    } else {
        workspace::WorkspaceIndex::build_incremental(&cwd, &cache_path, 1600, 256 * 1024)
    };
    app.status_line = format!(
        "Indexed {} files (skipped {})",
        index.indexed_files, index.skipped_files
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(index.built_unix);
    let age_secs = now.saturating_sub(index.built_unix);
    app.history.push(HistoryEntry {
        is_user: false,
        text: format!(
            "Workspace index built for `{}`\n- files indexed: {}\n- skipped: {}\n- age: {}s",
            index.root.display(),
            index.indexed_files,
            index.skipped_files,
            age_secs
        ),
    });

    app.workspace_index = Some(index);
}

fn enrich_with_workspace_context(app: &mut App, input: &str) -> String {
    if app.workspace_index.is_none() {
        build_workspace_index(app, false);
    }

    let Some(index) = app.workspace_index.as_ref() else {
        return input.to_string();
    };

    let matches = index.retrieve(input, 3, 5000);
    if matches.is_empty() {
        return input.to_string();
    }

    let mut enriched = input.to_string();
    enriched.push_str("\n\n[Retrieved workspace context]\n");
    for (path, snippet) in matches {
        enriched.push_str(&format!("\nFile: {}\n```text\n{}\n```\n", path, snippet));
    }
    enriched
}

fn enrich_with_attached_files(input: &str) -> String {
    const MAX_ATTACHMENTS: usize = 3;
    const MAX_FILE_BYTES: usize = 128 * 1024;
    const MAX_FILE_CHARS: usize = 20_000;

    let mut enriched = input.to_string();
    let mut attached = 0usize;
    let mut seen = std::collections::HashSet::new();

    for token in input.split_whitespace() {
        if !token.starts_with('@') {
            continue;
        }
        if attached >= MAX_ATTACHMENTS {
            break;
        }

        let raw = token
            .trim_start_matches('@')
            .trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';' || c == ')');
        if raw.is_empty() || !seen.insert(raw.to_string()) {
            continue;
        }

        let path = PathBuf::from(raw);
        if !path.exists() || !path.is_file() {
            continue;
        }

        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.len() as usize > MAX_FILE_BYTES {
            enriched.push_str(&format!(
                "\n\n[Attachment skipped: {} is larger than {} KB]",
                raw,
                MAX_FILE_BYTES / 1024
            ));
            continue;
        }

        let Ok(contents) = fs::read_to_string(&path) else {
            enriched.push_str(&format!(
                "\n\n[Attachment skipped: {} is not UTF-8 text]",
                raw
            ));
            continue;
        };

        let mut snippet = contents;
        if snippet.len() > MAX_FILE_CHARS {
            let mut end = MAX_FILE_CHARS;
            while end > 0 && !snippet.is_char_boundary(end) {
                end -= 1;
            }
            snippet.truncate(end);
            snippet.push_str("\n[...truncated]");
        }

        enriched.push_str(&format!(
            "\n\n[Attached file: {}]\n```text\n{}\n```",
            raw, snippet
        ));
        attached += 1;
    }

    enriched
}

fn push_error(app: &mut App, err: &str) {
    app.history.push(HistoryEntry {
        is_user: false,
        text: format!("**Error:** {}", err),
    });
}

fn build_local_plan(task: &str) -> Vec<String> {
    let mut steps = vec![
        format!("Clarify scope and expected outcome for: {}", task),
        "Inspect relevant files and current behavior".to_string(),
        "Implement smallest safe fix or feature slice".to_string(),
        "Run validation (build/tests/manual checks)".to_string(),
        "Summarize changes, risks, and next improvements".to_string(),
    ];

    let lowered = task.to_lowercase();
    if lowered.contains("performance") || lowered.contains("fast") {
        steps.insert(
            3,
            "Profile hot paths and reduce repeated work/calls before adding complexity".to_string(),
        );
    }
    if lowered.contains("ui") || lowered.contains("tui") || lowered.contains("ux") {
        steps.insert(
            3,
            "Refine layout, navigation shortcuts, and information density for coding workflows"
                .to_string(),
        );
    }

    steps
}
