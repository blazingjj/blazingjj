//! The event module is where events are generated. This means
//! capture keyboard and mouse events from user, as well as listening
//! for file system notifications in case somebody used jj on this
//! repository.

mod mouse;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub use mouse::CLICK_PAUSE;
pub use mouse::Clicks;
pub use mouse::Mouse;
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

/// The input reader thread, and the flag that keeps it reading.
struct Reader {
    reading: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

/// Generator of events to the app
pub struct EventSource {
    // Global shut down flag
    running: Arc<AtomicBool>,

    // The input reader, while there is one
    reader: Option<Reader>,

    // Channel for app events
    app_event_sender: mpsc::Sender<AppEvent>,
    app_event_receiver: mpsc::Receiver<AppEvent>,
}

impl EventSource {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            running,
            reader: None,
            app_event_sender: tx,
            app_event_receiver: rx,
        }
    }

    /// Stop the input reader and return once it is gone, so that a
    /// foreground process has the terminal and the user's keys to itself.
    pub fn pause_user_input(&mut self) {
        let Some(reader) = self.reader.take() else {
            return;
        };

        reader.reading.store(false, Ordering::Relaxed);
        if reader.thread.join().is_err() {
            error!("crossterm reader panicked");
        }
    }

    /// Read user input again, dropping everything the user typed at the
    /// foreground process in the meantime.
    pub fn resume_user_input(&mut self) {
        self.discard_user_input();
        self.launch_user_input();
    }

    /// Discard user input, keeping other events. What the user typed while
    /// the terminal belonged to someone else was not meant for us, whether
    /// it is still sitting in the terminal or already on our channel.
    fn discard_user_input(&mut self) {
        while let Ok(true) = crossterm::event::poll(Duration::ZERO) {
            if crossterm::event::read().is_err() {
                break;
            }
        }

        let keep: Vec<AppEvent> = self
            .app_event_receiver
            .try_iter()
            .filter(|event| !matches!(event, AppEvent::UserInput(_)))
            .collect();
        for event in keep {
            let _ = self.app_event_sender.send(event);
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
        let reading = Arc::new(AtomicBool::new(true));
        let app_event_tx = self.app_event_sender.clone();
        trace!("spawn crossterm reader");
        let thread = thread::Builder::new()
            .name("crossterm reader".to_string())
            .spawn({
                let reading = reading.clone();
                move || {
                    let poll_period = Duration::from_millis(100);
                    trace!("crossterm reader - started");
                    while running.load(Ordering::Relaxed) && reading.load(Ordering::Relaxed) {
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
                }
            })
            .unwrap();

        self.reader = Some(Reader { reading, thread });
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

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyEvent;

    use super::*;

    fn event_source() -> EventSource {
        EventSource::new(Arc::new(AtomicBool::new(true)))
    }

    #[test]
    fn pausing_without_a_reader_does_not_wait() {
        // There is nothing to stop before the reader has been launched.
        let (done, paused) = mpsc::channel();
        thread::spawn(move || {
            event_source().pause_user_input();
            done.send(())
        });

        assert!(paused.recv_timeout(Duration::from_secs(10)).is_ok());
    }

    #[test]
    fn what_was_typed_at_the_foreground_process_is_discarded() {
        let mut source = event_source();
        let sender = source.clone_event_sender();
        let key = crossterm::event::Event::Key(KeyEvent::from(KeyCode::Char('q')));
        sender.send(AppEvent::UserInput(key)).unwrap();

        source.discard_user_input();

        assert!(source.try_recv(Duration::ZERO).is_none());
    }
}
