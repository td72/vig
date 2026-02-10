use anyhow::Result;
use crossterm::event::{self, KeyEvent};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
#[allow(dead_code)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    Resize(u16, u16),
    FsChange,
}

pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
    _tx: mpsc::Sender<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();
        thread::spawn(move || loop {
            if event::poll(tick_rate).unwrap_or(false) {
                match event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        if event_tx.send(Event::Key(key)).is_err() {
                            return;
                        }
                    }
                    Ok(crossterm::event::Event::Resize(w, h)) => {
                        if event_tx.send(Event::Resize(w, h)).is_err() {
                            return;
                        }
                    }
                    _ => {}
                }
            } else if event_tx.send(Event::Tick).is_err() {
                return;
            }
        });
        Self { rx, _tx: tx }
    }

    pub fn tx(&self) -> mpsc::Sender<Event> {
        self._tx.clone()
    }

    pub fn next(&self) -> Result<Event> {
        Ok(self.rx.recv()?)
    }
}
