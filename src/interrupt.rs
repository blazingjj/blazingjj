//! Surviving a Ctrl-C that was meant for a process we put in front of the
//! user.

use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use anyhow::Result;

/// Whether a SIGINT still ends the process, which it does unless someone
/// else has the terminal.
static ENDS_US: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(true)));

/// Route SIGINT through us rather than straight to the default of ending
/// the process, so that [catch_interrupts] can hold it back. Until it
/// does, the signal goes on ending us as it always has.
///
/// Windows delivers Ctrl-C to every process on the console rather than to
/// a process group, and taking it over there needs a console control
/// handler we do not have, so this does nothing and a Ctrl-C aimed at a
/// foreground process still ends the app.
pub fn watch_for_interrupts() -> Result<()> {
    #[cfg(unix)]
    signal_hook::flag::register_conditional_default(signal_hook::consts::SIGINT, ENDS_US.clone())?;

    Ok(())
}

/// Keep a Ctrl-C from ending the process for as long as the returned guard
/// lives, leaving it to whoever we handed the terminal to.
pub fn catch_interrupts() -> CaughtInterrupts {
    ENDS_US.store(false, Ordering::Relaxed);
    CaughtInterrupts
}

/// Lets SIGINT end the process again when dropped.
pub struct CaughtInterrupts;

impl Drop for CaughtInterrupts {
    fn drop(&mut self) {
        ENDS_US.store(true, Ordering::Relaxed);
    }
}
