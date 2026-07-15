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
use std::time::Instant;

use ratatui::crossterm;
use tracing::error;
use tracing::trace;

/// Minimum time between idle-events
const IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// Input event to the app
#[derive(Debug,PartialEq)]
pub enum AppEvent {
    /// Keyboard or mouse input from user
    UserInput(crossterm::event::Event),
}

/// Generator of events to the app
pub struct EventSource {
    // Global shut down flag
    running: Arc<AtomicBool>,

    // Channel for app events
    app_event_sender: mpsc::Sender<AppEvent>,
    app_event_receiver: mpsc::Receiver<AppEvent>,

    // Consumer data
    last_event_recv: Instant,
    last_event_none: bool,
}

impl EventSource {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            running,
            app_event_sender: tx,
            app_event_receiver: rx,
            last_event_recv: Instant::now(),
            last_event_none: false,
        }
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
                        break; }
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
    /// If no event is waiting, it will return None which represents
    /// an idle event. There will be at least IDLE_TIMEOUT between two
    /// consecutive idle events. Ordinary events are returned immediately.
    pub fn try_recv(&mut self) -> Option<AppEvent> {
        // Introduce timeout if app is idle.
        // This will reduce CPU load
        let timeout = if self.last_event_none {
            IDLE_TIMEOUT
        } else {
            Duration::ZERO
        };

        // Get event
        let result = loop {
            // Wait for event. While waiting the watcher thread is allowed to
            // trigger a redraw.
            let result = self.app_event_receiver.recv_timeout(timeout);
            break result;
        };

        // Check for app event
        if let Ok(event) = result {
            trace!("try_recv - received app event");
            self.last_event_recv = Instant::now();
            self.last_event_none = false;
            return Some(event);
        }

        // No event found. This will trigger a redraw in the main loop
        self.last_event_none = true;
        trace!("try_recv - no event");
        None
    }
}
