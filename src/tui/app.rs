use std::time::Instant;

use ratatui::style::Style;
use tui_textarea::TextArea;

use crate::tui::workspace::WorkspaceIndex;

#[derive(Debug, Clone)]
pub enum TuiMessage {
    Token(String),
    StreamFinished {
        latency_ms: u128,
        output_chars: usize,
    },
    Error(String),
    WarmupStatus(String),
    WorkspaceIndexReady {
        index: WorkspaceIndex,
        source: String,
    },
    CommandStarted {
        pid: u32,
        cmd: String,
    },
    CommandStream(String),
    CommandOutput {
        cmd: String,
        output: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Chat,
    Processing,
    Settings,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub is_user: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Input,
    Output,
}

pub struct App<'a> {
    pub route: Route,
    pub input_buffer: TextArea<'a>,
    pub is_loading: bool,
    pub spinner_state: u8,
    pub exit: bool,
    pub result_buffer: String,
    pub streaming_buffer: String,
    pub result_scroll: u16,
    pub streaming_scroll: u16,
    pub focus: Focus,
    pub history: Vec<HistoryEntry>,
    pub pasted_code: Option<String>,
    pub last_key_time: Instant,
    pub show_help: bool,
    pub status_line: String,
    pub rag_enabled: bool,
    pub workspace_index: Option<WorkspaceIndex>,
    pub pending_command: Option<String>,
    pub command_running: bool,
    pub command_pid: Option<u32>,
    pub planner_steps: Vec<String>,
    pub planner_cursor: usize,
    pub total_responses: u64,
    pub total_output_chars: u64,
    pub last_latency_ms: Option<u128>,
}

impl<'a> Default for App<'a> {
    fn default() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type a message or paste code...");
        textarea.set_cursor_line_style(Style::default()); // No highlight line by default

        Self {
            route: Route::Chat,
            input_buffer: textarea,
            is_loading: false,
            spinner_state: 0,
            exit: false,
            result_buffer: String::new(),
            streaming_buffer: String::new(),
            result_scroll: 0,
            streaming_scroll: 0,
            focus: Focus::Input,
            history: Vec::new(),
            pasted_code: None,
            last_key_time: std::time::Instant::now(),
            show_help: false,
            status_line: "Ready".to_string(),
            rag_enabled: true,
            workspace_index: None,
            pending_command: None,
            command_running: false,
            command_pid: None,
            planner_steps: Vec::new(),
            planner_cursor: 0,
            total_responses: 0,
            total_output_chars: 0,
            last_latency_ms: None,
        }
    }
}

impl<'a> App<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_route(&mut self, route: Route) {
        self.route = route;
        self.input_buffer = TextArea::default();
        self.input_buffer.set_cursor_line_style(Style::default());
        self.streaming_buffer.clear();
        self.result_buffer.clear();
        self.result_scroll = 0;
        self.streaming_scroll = 0;
        self.focus = Focus::Input;
        self.pasted_code = None;

        match self.route {
            Route::Chat => self
                .input_buffer
                .set_placeholder_text("Type a message or paste code (Double Enter to submit)..."),
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        if self.is_loading {
            self.spinner_state = self.spinner_state.wrapping_add(1);
        }
    }
}
