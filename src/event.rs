//! The event module is where events are generated. This means
//! capture keyboard and mouse events from user, as well as listening
//! for file system notifications in case somebody used jj on this
//! repository.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::crossterm;
use tracing::error;
use tracing::trace;

use crate::background_tasks::TaskResult;

/// Input event to the app
#[derive(Debug)]
pub enum AppEvent {
    /// Keyboard or mouse input from user
    UserInput(crossterm::event::Event),
    /// A background task finished and delivers its output
    TaskDone(TaskResult),
}

/// Generator of events to the app
pub struct EventSource {
    // Global shut down flag
    running: Arc<AtomicBool>,

    // Channel for app events
    app_event_sender: mpsc::Sender<AppEvent>,
    app_event_receiver: mpsc::Receiver<AppEvent>,
}

impl EventSource {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            running,
            app_event_sender: tx,
            app_event_receiver: rx,
        }
    }

    /// Clone the sender to the event channel
    pub fn clone_event_sender(&self) -> mpsc::Sender<AppEvent> {
        self.app_event_sender.clone()
    }

    /// Launch a user input event source
    pub fn launch_user_input(&mut self) {
        // Spawn user input thread
        let running = self.running.clone();
        let app_event_tx = self.app_event_sender.clone();
        trace!("spawn crossterm reader");
        thread::Builder::new()
            .name("crossterm reader".to_string())
            .spawn(move || {
                let poll_period = Duration::from_millis(100);
                trace!("crossterm reader - started");
                while running.load(Ordering::Relaxed) {
                    // Block until an event arrives
                    match crossterm::event::poll(poll_period) {
                        Err(err) => {
                            error!("cossterm reader - poll abort: {:?}", err);
                            break;
                        }
                        Ok(false) => continue, // No event yet
                        Ok(true) => (),
                    }
                    let Ok(event) = crossterm::event::read() else {
                        error!("crossterm reader - read abort");
                        break;
                    };
                    // Send event to main thread
                    let Ok(_) = app_event_tx.send(AppEvent::UserInput(event)) else {
                        error!("crossterm reader - send abort");
                        break;
                    };
                    trace!("crossterm reader - event forwarded");
                }
                trace!("crossterm reader - stopped");
            })
            .unwrap();
    }

    /// Receive an AppEvent if one is waiting.
    /// If no event arrives within the timeout, it will return None which
    /// represents an idle event, triggering a redraw in the main loop.
    pub fn try_recv(&self, timeout: Duration) -> Option<AppEvent> {
        match self.app_event_receiver.recv_timeout(timeout) {
            Ok(event) => {
                trace!("try_recv - received app event");
                Some(event)
            }
            Err(_) => {
                trace!("try_recv - no event");
                None
            }
        }
    }
}
