/*! Giving up on a command that is still running */

use std::process::Child;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

use tracing::error;

/// A handle for killing a command while it runs. The command registers
/// the child process it is waiting for, so a token nothing registered on
/// has nothing to kill.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Mutex<TokenState>>);

#[derive(Default)]
struct TokenState {
    cancelled: bool,
    child: Option<Child>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hand over the child process the command is waiting for. Panics if a
    /// child is still registered, as a command registers one at a time and
    /// takes it back with [Self::take_child] to reap it.
    pub fn register(&self, child: Child) {
        let mut state = self.lock();
        assert!(
            state.child.is_none(),
            "a command registered a second child without reaping the first"
        );
        state.child = Some(child);
        // Cancellation may have happened before the child existed.
        if state.cancelled {
            state.kill();
        }
    }

    /// Take the child back to reap it. Killing only happens while the
    /// child is registered, so a child taken back here is never killed
    /// afterwards -- and never reaped twice.
    pub fn take_child(&self) -> Option<Child> {
        self.lock().child.take()
    }

    /// Whether we gave up on the command, making its output worthless.
    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Give up on the command, killing it if it is already running.
    pub fn cancel(&self) {
        let mut state = self.lock();
        state.cancelled = true;
        state.kill();
    }

    fn lock(&self) -> MutexGuard<'_, TokenState> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl TokenState {
    /// Kill the registered child, if any. Leaves it to be reaped.
    fn kill(&mut self) {
        if let Some(child) = self.child.as_mut()
            && let Err(err) = child.kill()
        {
            error!("Failed to kill child process of cancelled command: {err}");
        }
    }
}

impl Drop for TokenState {
    /// Kills and reaps a child the command never took back, which the
    /// [Child] its own drop does neither of.
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        error!("Reaping the child process of a command that did not finish");
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::commander::CommandError;
    use crate::commander::tests::TestRepo;

    /// A child process that keeps running until something kills it.
    fn sleeping_child() -> Child {
        Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep")
    }

    /// Reap a child the token was supposed to kill.
    fn assert_killed(mut child: Child) {
        let status = child.wait().expect("failed to wait for child");
        assert!(!status.success(), "a killed child does not exit cleanly");
    }

    #[test]
    fn kills_the_child_of_a_cancelled_command() {
        let cancel = CancelToken::new();
        cancel.register(sleeping_child());

        cancel.cancel();

        assert!(cancel.is_cancelled());
        assert_killed(cancel.take_child().expect("the child was registered"));
    }

    #[test]
    fn kills_a_child_registered_after_cancellation() {
        // We can give up on a command before it has even been spawned, so
        // the kill has to happen on registration instead.
        let cancel = CancelToken::new();
        cancel.cancel();

        cancel.register(sleeping_child());

        assert_killed(cancel.take_child().expect("the child was registered"));
    }

    #[test]
    fn leaves_a_child_it_handed_back_alone() {
        // A command takes its child back to reap it, and a kill arriving
        // after that must not reach an unrelated process reusing the pid.
        let cancel = CancelToken::new();
        cancel.register(sleeping_child());
        let mut child = cancel.take_child().expect("the child was registered");

        cancel.cancel();

        assert!(
            child
                .try_wait()
                .expect("failed to check on child")
                .is_none(),
            "the child should still be running"
        );
        child.kill().expect("failed to kill child");
        child.wait().expect("failed to wait for child");
    }

    #[test]
    fn reports_a_command_killed_through_its_token() -> anyhow::Result<()> {
        let test_repo = TestRepo::new()?;
        // Cancelling up front makes the kill land the moment the command
        // registers its child, rather than racing a command that is done
        // in milliseconds.
        let cancel = CancelToken::new();
        cancel.cancel();

        let error = test_repo
            .commander
            .jj(["log"])
            .run_cancellable(&cancel)
            .expect_err("a killed command has no output to return");

        assert!(matches!(error, CommandError::Status(..)));

        Ok(())
    }
}
