/*! Signal some time in the future */

use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/** Timer to emit a specific signal after some period. Can be cancelled.

Goals:
    Launch background thread once at applicatoin start
    The app will request a signal after some period several times.
    Any request for a signal will overwrite a previous request.
    It is possible to cancel the current request.
    When the application closes it should join the background thread

*/
pub struct Timer<Signal> {
    cmd_sender: Sender<TimerCommand<Signal>>,
    thread: Option<thread::JoinHandle<()>>,
}

/// Commands sent from the main thread to the background timer thread
enum TimerCommand<Signal> {
    Set(Signal, Instant),
    Cancel,
    Stop,
}

/** User interface from other thread */
impl<Signal: Send + 'static> Timer<Signal> {
    pub fn new(app_sender: Sender<Signal>) -> Self {
        let (cmd_sender, cmd_receiver) = channel::<TimerCommand<Signal>>();

        let thread = thread::Builder::new()
            .name("Timer".to_string())
            .spawn(move || Self::timer_loop(cmd_receiver, app_sender))
            .expect("Failed to spawn timer thread");

        Self {
            cmd_sender,
            thread: Some(thread),
        }
    }
    /// The timer thread loop that waits and sends the signal
    fn timer_loop(cmd_receiver: Receiver<TimerCommand<Signal>>, app_sender: Sender<Signal>) {
        let mut current_task: Option<(Signal, Instant)> = None;

        loop {
            // 1. Determine how long to wait
            let timeout = current_task.as_ref().map(|(_, at)| {
                at.checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO)
            });

            // 2. Wait for a new command or the current timer to expire
            let cmd = match timeout {
                Some(duration) => cmd_receiver.recv_timeout(duration).ok(),
                None => Some(cmd_receiver.recv().unwrap_or(TimerCommand::Stop)),
            };

            match cmd {
                Some(TimerCommand::Set(sig, at)) => {
                    current_task = Some((sig, at));
                }
                Some(TimerCommand::Cancel) => {
                    current_task = None;
                }
                Some(TimerCommand::Stop) | None => break,
            }

            // 3. If a task exists and the time has passed, send the signal
            if let Some((_, at)) = current_task
                && Instant::now() >= at
                && let Some((sig, _)) = current_task.take()
            {
                let _ = app_sender.send(sig);
            }

            thread::sleep(Duration::from_millis(1));
        }
    }
    /// Terminate timer thread and wait for it to join
    pub fn stop(&mut self) {
        let _ = self.cmd_sender.send(TimerCommand::Stop);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
    /// Queue a signal
    pub fn signal_in(&self, signal: Signal, wait_period: Duration) {
        let _ = self
            .cmd_sender
            .send(TimerCommand::Set(signal, Instant::now() + wait_period));
    }
    /// Cancel next signal
    pub fn cancel_signal(&self) {
        let _ = self.cmd_sender.send(TimerCommand::Cancel);
    }
}
