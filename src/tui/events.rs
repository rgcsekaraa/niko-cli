use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};

use crate::tui::app::TuiMessage;

#[derive(Debug)]
pub enum Event {
    Tick,
    Key(KeyEvent),
    Paste(String),
    Resize,
    AppMessage(TuiMessage),
}

pub struct EventHandler {
    pub sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    #[allow(dead_code)] // Handler is detached but kept in struct
    handler: thread::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();
        let handler_sender = sender.clone();

        let handler = thread::spawn(move || {
            let mut last_tick = Instant::now();
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or_else(|| Duration::from_secs(0));

                if event::poll(timeout).expect("failed to poll new events") {
                    match event::read().expect("unable to read event") {
                        CrosstermEvent::Key(e) => {
                            if e.kind == event::KeyEventKind::Press {
                                handler_sender
                                    .send(Event::Key(e))
                                    .expect("failed to send key event");
                            }
                        }
                        CrosstermEvent::Paste(s) => {
                            handler_sender
                                .send(Event::Paste(s))
                                .expect("failed to send paste event");
                        }
                        CrosstermEvent::Resize(_, _) => {
                            handler_sender
                                .send(Event::Resize)
                                .expect("failed to send resize event");
                        }
                        _ => {}
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    handler_sender
                        .send(Event::Tick)
                        .expect("failed to send tick event");
                    last_tick = Instant::now();
                }
            }
        });

        Self {
            sender,
            receiver,
            handler,
        }
    }

    pub fn next(&self) -> Result<Event> {
        Ok(self.receiver.recv()?)
    }
}
