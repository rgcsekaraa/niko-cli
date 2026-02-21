use std::time::Instant;

use ratatui::{style::Style, widgets::ListState};
use tui_textarea::TextArea;
// use crate::chunker::ExplainResult;
// cyclic dependency if I import ExplainResult here? No, app.rs is part of tui mod.
// But ExplainResult is in chunker.rs.
// I need `use crate::chunker::ExplainResult;`

use crate::chunker::ExplainResult;

#[derive(Debug, Clone)]
pub enum TuiMessage {
    Token(String),
    CmdResult(Result<String, String>),
    ExplainResult(Result<ExplainResult, String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Menu,
    Main,      // Unified view for Cmd, Explain, etc.
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
    pub menu_state: ListState,
    pub pasted_code: Option<String>,
    pub last_key_time: Instant,
}

impl<'a> Default for App<'a> {
    fn default() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type something...");
        textarea.set_cursor_line_style(Style::default()); // No highlight line by default

        let mut menu_state = ListState::default();
        menu_state.select(Some(0)); // default to first item

        Self {
            route: Route::Menu,
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
            menu_state,
            pasted_code: None,
            last_key_time: std::time::Instant::now(),
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
            Route::Main => self
                .input_buffer
                .set_placeholder_text("Type a command or paste code (Ctrl+D or Enter)..."),
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        if self.is_loading {
            self.spinner_state = self.spinner_state.wrapping_add(1);
        }
    }
}
