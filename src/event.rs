use anyhow::Result;
use crossterm::event::{self, KeyEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
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
    paused: Arc<AtomicBool>,
    pause_ack: Arc<(Mutex<bool>, Condvar)>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();
        let paused = Arc::new(AtomicBool::new(false));
        let paused_flag = Arc::clone(&paused);
        let pause_ack: Arc<(Mutex<bool>, Condvar)> =
            Arc::new((Mutex::new(false), Condvar::new()));
        let ack_clone = Arc::clone(&pause_ack);
        thread::spawn(move || loop {
            if paused_flag.load(Ordering::SeqCst) {
                // Signal that we have entered the paused state
                {
                    let (lock, cvar) = &*ack_clone;
                    let mut acked = lock.lock().unwrap();
                    *acked = true;
                    cvar.notify_one();
                }
                // Spin-wait with short sleeps until resumed
                while paused_flag.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(10));
                }
                continue;
            }
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
        Self {
            rx,
            _tx: tx,
            paused,
            pause_ack,
        }
    }

    pub fn tx(&self) -> mpsc::Sender<Event> {
        self._tx.clone()
    }

    pub fn next(&self) -> Result<Event> {
        Ok(self.rx.recv()?)
    }

    /// Pause event polling. Blocks until the background thread has actually
    /// stopped calling `crossterm::event::poll()`/`read()`.
    pub fn pause(&self) {
        // Reset ack flag
        {
            let (lock, _) = &*self.pause_ack;
            *lock.lock().unwrap() = false;
        }
        self.paused.store(true, Ordering::SeqCst);
        // Wait for the thread to acknowledge it has entered the paused state
        let (lock, cvar) = &*self.pause_ack;
        let mut acked = lock.lock().unwrap();
        while !*acked {
            acked = cvar.wait(acked).unwrap();
        }
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }
}
