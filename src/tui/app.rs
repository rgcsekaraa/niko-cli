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
    Finished,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Menu,
    CmdInput,
    ExplainInput,
    Processing,
    ResultView,
    Settings,
}

pub struct App<'a> {
    pub route: Route,
    pub input_buffer: TextArea<'a>,
    pub messages: Vec<String>,
    pub is_loading: bool,
    pub spinner_state: u8,
    pub exit: bool,
    pub result_buffer: String,
    pub streaming_buffer: String, // New field for streaming token accumulator
}

impl<'a> Default for App<'a> {
    fn default() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type something...");

        Self {
            route: Route::Menu,
            input_buffer: textarea,
            messages: Vec::new(),
            is_loading: false,
            spinner_state: 0,
            exit: false,
            result_buffer: String::new(),
            streaming_buffer: String::new(),
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
        self.streaming_buffer.clear(); // Clear streaming buffer on route change
        self.result_buffer.clear();

        match self.route {
            Route::CmdInput => self
                .input_buffer
                .set_placeholder_text("Describe the command (e.g., 'find large files')..."),
            Route::ExplainInput => self
                .input_buffer
                .set_placeholder_text("Paste code here (Ctrl+D or Enter to submit)..."),
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        if self.is_loading {
            self.spinner_state = self.spinner_state.wrapping_add(1);
        }
    }
}
