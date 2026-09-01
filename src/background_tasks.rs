/*! Background execution of jj commands.

Each task gets a thread that blocks until its command is done and then
sends the result into the [AppEvent] channel the main loop waits on, so
no caller ever blocks on a jj invocation or has to ask whether one is
finished.

A task is identified by a [TaskSlot]. The slot is both the deduplication
key -- submitting work that is already in flight is a no-op -- and the
label that lets the consumer recognize what it gets back.
*/

use std::fmt;
use std::io;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::mpsc::Sender;
use std::thread;

use thiserror::Error;
use tracing::debug;
use tracing::error;

use crate::app::TabId;
use crate::commander::CommandError;
use crate::commander::cancel::CancelToken;
use crate::event::AppEvent;
use crate::ui::files_tab::FileDiffKey;
use crate::ui::panel::CommitShowKey;
use crate::ui::panel::EvologShowKey;
use crate::ui::panel::OpShowKey;
use crate::ui::panel::OutputRequest;

/// How many tasks may run at once. A submission beyond this makes room by
/// killing the ones that have been running longest.
const MAX_RUNNING: usize = 8;

/// The output of a task.
pub type TaskOutput = Result<String, TaskError>;

/// Why a task has no output to deliver.
#[derive(Debug, Error)]
pub enum TaskError {
    #[error(transparent)]
    Command(#[from] CommandError),
    /// The task never got as far as running its command, so there is no
    /// status and no output to report.
    #[error("Failed to start background task: {0}")]
    Spawn(io::Error),
    /// The task panicked on the way, so there is no status and no output
    /// to report.
    #[error("Background task panicked")]
    Panic,
}

/// What a task is for. A slot dedups submissions of work already in
/// flight, and lets the consumer tell whether the result it receives is
/// still the one it wants to show.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskSlot {
    /// A 'jj show' for the details panel of a tab. Two tabs may want the
    /// same change at once, and each keeps its own copy of the output, so
    /// the tab is part of the slot.
    CommitShow(TabId, OutputRequest<CommitShowKey>),
    /// A 'jj diff' for the details panel of a tab
    FileDiff(TabId, OutputRequest<FileDiffKey>),
    /// A 'jj evolog' of a single entry for the details panel of a tab
    EvologShow(TabId, OutputRequest<EvologShowKey>),
    /// A 'jj op show' for the details panel of a tab
    OpShow(TabId, OutputRequest<OpShowKey>),
    GitPush,
    GitFetch,
    /// A read of what operation the repo is at.
    RepoOpId,
}

/// Which run of a slot a task is. Unique for the lifetime of the app,
/// so a result never stands for another run of the same slot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TaskId(u64);

/// What a finished task delivers to the main loop.
pub struct TaskResult {
    pub slot: TaskSlot,
    pub output: TaskOutput,
    id: TaskId,
}

impl fmt::Debug for TaskResult {
    /// Reports the size of the output rather than the document itself.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskResult")
            .field("slot", &self.slot)
            .field("id", &self.id)
            .field("output_len", &self.output.as_ref().map(String::len))
            .finish()
    }
}

/// A handle for submitting background work. Cloning gives another handle
/// to the same tasks, so each component can submit its own.
#[derive(Clone)]
pub struct BackgroundTasks {
    app_event_sender: Sender<AppEvent>,
    running: Arc<Mutex<Running>>,
}

/// What we have in flight.
#[derive(Default)]
struct Running {
    /// The tasks, in submission order.
    tasks: Vec<RunningTask>,
    next_id: u64,
}

/// A task we have started and not yet seen the result of.
struct RunningTask {
    id: TaskId,
    slot: TaskSlot,
    cancel: CancelToken,
    /// Whether we may kill this task to make room for newer work.
    evictable: bool,
}

impl BackgroundTasks {
    pub fn new(app_event_sender: Sender<AppEvent>) -> Self {
        Self {
            app_event_sender,
            running: Arc::new(Mutex::new(Running::default())),
        }
    }

    /// Release the slot of a task whose result has arrived, so the same
    /// work can be submitted again.
    pub fn finish(&self, result: &TaskResult) {
        self.take(|task| task.id == result.id);
    }

    /// Give up on the task in `slot`, killing the command it waits for and
    /// discarding whatever it has produced. Does nothing if that slot is
    /// not running.
    pub fn cancel(&self, slot: &TaskSlot) {
        let Some(cancelled) = self.take(|task| &task.slot == slot) else {
            return;
        };
        debug!("Cancelling task: {:?}", cancelled.slot);
        cancelled.cancel.cancel();
    }

    /// Take the first task `predicate` accepts out of the registry.
    fn take(&self, predicate: impl Fn(&RunningTask) -> bool) -> Option<RunningTask> {
        let mut running = self.lock();
        let index = running.tasks.iter().position(predicate)?;
        Some(running.tasks.remove(index))
    }

    /// Run `task` in the background, unless the same work is already in
    /// flight. If we later need room for newer work, the task may be
    /// killed, in which case its result is discarded.
    pub fn submit<F>(&self, slot: TaskSlot, task: F)
    where
        F: FnOnce(&CancelToken) -> TaskOutput + Send + 'static,
    {
        self.start(slot, true, task);
    }

    /// Like [Self::submit], but the task keeps its slot until it is done
    /// and is never killed to make room.
    pub fn submit_uninterruptible<F>(&self, slot: TaskSlot, task: F)
    where
        F: FnOnce() -> TaskOutput + Send + 'static,
    {
        self.start(slot, false, move |_cancel| task());
    }

    fn start<F>(&self, slot: TaskSlot, evictable: bool, task: F)
    where
        F: FnOnce(&CancelToken) -> TaskOutput + Send + 'static,
    {
        let mut running = self.lock();

        // The result of the task already running is exactly what a second
        // submission would compute.
        if running.tasks.iter().any(|other| other.slot == slot) {
            return;
        }

        evict_for_room(&mut running.tasks);

        let id = TaskId(running.next_id);
        running.next_id += 1;
        let cancel = CancelToken::new();
        let thread_cancel = cancel.clone();
        let thread_slot = slot.clone();
        let sender = self.app_event_sender.clone();
        let thread = thread::Builder::new()
            .name("background task".to_owned())
            .spawn(move || {
                // A panicking task must neither take the app down nor
                // leave the consumer waiting for a result forever.
                let output = catch_unwind(AssertUnwindSafe(|| task(&thread_cancel)))
                    .unwrap_or(Err(TaskError::Panic));

                // A killed command leaves a truncated document behind,
                // which must not masquerade as the real output.
                if thread_cancel.is_cancelled() {
                    debug!("Discarding output of cancelled task");
                    return;
                }

                let result = TaskResult {
                    slot: thread_slot,
                    output,
                    id,
                };
                let _ = sender.send(AppEvent::TaskDone(result));
            });

        match thread {
            Ok(_) => running.tasks.push(RunningTask {
                id,
                slot,
                cancel,
                evictable,
            }),
            // The consumer is waiting for this slot, so it has to hear
            // about the failure rather than wait for a result forever.
            Err(err) => {
                error!("Failed to spawn task thread: {err}");
                let result = TaskResult {
                    slot,
                    output: Err(TaskError::Spawn(err)),
                    id,
                };
                let _ = self.app_event_sender.send(AppEvent::TaskDone(result));
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Running> {
        // Only the submitting thread ever holds this lock, and it does
        // nothing under it that can panic, so a poisoned lock still
        // protects consistent state.
        self.running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether the task in `slot` has yet to deliver its result.
    pub fn is_running(&self, slot: &TaskSlot) -> bool {
        self.lock().tasks.iter().any(|task| &task.slot == slot)
    }

    /// Whether a submission would find a free slot rather than have to
    /// make room.
    pub fn has_room(&self) -> bool {
        self.lock().tasks.len() < MAX_RUNNING
    }

    #[cfg(test)]
    fn running_count(&self) -> usize {
        self.lock().tasks.len()
    }
}

/// Kill tasks that may be killed until there is room for one more. The
/// tasks are in submission order, so the first one that may be killed has
/// had the longest to produce something and is the one we can least expect
/// to still be wanted. Leaves us over [MAX_RUNNING] when none of them may
/// be killed.
fn evict_for_room(running: &mut Vec<RunningTask>) {
    while running.len() >= MAX_RUNNING {
        let Some(index) = running.iter().position(|task| task.evictable) else {
            break;
        };

        let task = running.remove(index);
        debug!("Killing slow task to make room: {:?}", task.slot);
        task.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::sync::mpsc::Receiver;
    use std::time::Duration;

    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;
    use crate::env::DiffFormat;
    use crate::ui::panel::OutputKey;

    /// Long enough that a missing result is a failure, not a slow machine.
    const DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);
    /// Long enough that a result which is not supposed to arrive would
    /// have arrived by now.
    const SILENCE_TIMEOUT: Duration = Duration::from_millis(200);

    fn tasks() -> (BackgroundTasks, Receiver<AppEvent>) {
        let (sender, receiver) = mpsc::channel();
        (BackgroundTasks::new(sender), receiver)
    }

    /// A distinct slot per index, so a test can fill every slot.
    fn slot(index: usize) -> TaskSlot {
        let head = Head {
            change_id: ChangeId(format!("change{index}")),
            commit_id: CommitId(format!("commit{index}")),
            divergent: false,
            immutable: false,
        };
        TaskSlot::CommitShow(
            TabId::Log,
            OutputRequest::new(CommitShowKey::new(head, DiffFormat::Git), 0),
        )
    }

    /// A task that occupies its slot until the test drops the returned
    /// sender.
    fn gated_task() -> (
        mpsc::Sender<()>,
        impl FnOnce() -> TaskOutput + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel::<()>();
        (sender, move || {
            let _ = receiver.recv();
            Ok("gated".to_owned())
        })
    }

    /// Fill every slot with tasks that block until the returned gates are
    /// dropped.
    fn fill_with_gated_tasks(tasks: &BackgroundTasks) -> Vec<mpsc::Sender<()>> {
        (0..MAX_RUNNING)
            .map(|index| {
                let (gate, task) = gated_task();
                tasks.submit(slot(index), move |_cancel| task());
                gate
            })
            .collect()
    }

    fn next_result(receiver: &Receiver<AppEvent>) -> TaskResult {
        let event = receiver
            .recv_timeout(DELIVERY_TIMEOUT)
            .expect("task result was not delivered");
        let AppEvent::TaskDone(result) = event else {
            panic!("expected a task result");
        };
        result
    }

    fn collect_slots(receiver: &Receiver<AppEvent>, count: usize) -> Vec<TaskSlot> {
        (0..count).map(|_| next_result(receiver).slot).collect()
    }

    #[test]
    fn delivers_result_on_the_event_channel() {
        let (tasks, receiver) = tasks();

        tasks.submit(slot(0), |_cancel| Ok("output".to_owned()));

        let result = next_result(&receiver);
        assert_eq!(result.slot, slot(0));
        assert_eq!(result.output.unwrap(), "output");
    }

    #[test]
    fn ignores_submission_of_work_already_in_flight() {
        let (tasks, receiver) = tasks();
        let runs = Arc::new(AtomicUsize::new(0));

        let (gate, blocked) = gated_task();
        let first_runs = runs.clone();
        tasks.submit(slot(0), move |_cancel| {
            first_runs.fetch_add(1, Ordering::SeqCst);
            blocked()
        });

        let second_runs = runs.clone();
        tasks.submit(slot(0), move |_cancel| {
            second_runs.fetch_add(1, Ordering::SeqCst);
            Ok("second".to_owned())
        });
        assert_eq!(tasks.running_count(), 1);

        drop(gate);
        assert_eq!(next_result(&receiver).output.unwrap(), "gated");
        assert!(receiver.recv_timeout(SILENCE_TIMEOUT).is_err());
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn delivers_result_of_a_stale_slot() {
        // We do not know what the consumer is looking at now, so a result
        // it has moved on from still arrives and can be cached.
        let (tasks, receiver) = tasks();

        tasks.submit(slot(0), |_cancel| Ok("stale".to_owned()));
        tasks.submit(slot(1), |_cancel| Ok("wanted".to_owned()));

        let delivered = collect_slots(&receiver, 2);
        assert!(delivered.contains(&slot(0)));
        assert!(delivered.contains(&slot(1)));
    }

    #[test]
    fn makes_room_by_killing_the_oldest_task() {
        let (tasks, _receiver) = tasks();
        let mut gates = fill_with_gated_tasks(&tasks);

        let (gate, task) = gated_task();
        gates.push(gate);
        tasks.submit(slot(MAX_RUNNING), move |_cancel| task());

        assert!(!tasks.is_running(&slot(0)), "oldest task should be evicted");
        assert!(
            tasks.is_running(&slot(1)),
            "younger tasks should be left be"
        );
        assert_eq!(tasks.running_count(), MAX_RUNNING);
    }

    #[test]
    fn never_evicts_an_uninterruptible_task() {
        let (tasks, _receiver) = tasks();
        let mut gates: Vec<_> = (0..MAX_RUNNING)
            .map(|index| {
                let (gate, task) = gated_task();
                tasks.submit_uninterruptible(slot(index), task);
                gate
            })
            .collect();

        let (gate, task) = gated_task();
        gates.push(gate);
        tasks.submit(slot(MAX_RUNNING), move |_cancel| task());

        assert_eq!(tasks.running_count(), MAX_RUNNING + 1);
        assert!(tasks.is_running(&slot(0)));
    }

    #[test]
    fn evicts_the_oldest_task_behind_an_uninterruptible_one() {
        let (tasks, _receiver) = tasks();
        // A push is the oldest task, so making room has to look past it
        // instead of giving up on the one task it may not kill.
        let (push_gate, push) = gated_task();
        tasks.submit_uninterruptible(TaskSlot::GitPush, push);
        let mut gates: Vec<_> = (1..MAX_RUNNING)
            .map(|index| {
                let (gate, task) = gated_task();
                tasks.submit(slot(index), move |_cancel| task());
                gate
            })
            .collect();
        gates.push(push_gate);

        let (gate, task) = gated_task();
        gates.push(gate);
        tasks.submit(slot(MAX_RUNNING), move |_cancel| task());

        assert!(
            tasks.is_running(&TaskSlot::GitPush),
            "a push is never killed"
        );
        assert!(!tasks.is_running(&slot(1)), "oldest task should be evicted");
        assert_eq!(tasks.running_count(), MAX_RUNNING);
    }

    #[test]
    fn discards_the_result_of_a_cancelled_task() {
        let (tasks, receiver) = tasks();
        let gates = fill_with_gated_tasks(&tasks);

        tasks.submit(slot(MAX_RUNNING), |_cancel| Ok("newest".to_owned()));
        assert!(!tasks.is_running(&slot(0)), "oldest task should be evicted");

        // The evicted task runs to completion here, since a plain closure
        // has nothing we could kill. Its result is dropped anyway.
        drop(gates);
        let delivered = collect_slots(&receiver, MAX_RUNNING);
        assert!(!delivered.contains(&slot(0)));
    }

    #[test]
    fn cancelling_frees_the_slot_and_drops_the_result() {
        let (tasks, receiver) = tasks();
        let (gate, task) = gated_task();
        tasks.submit(slot(0), move |_cancel| task());

        tasks.cancel(&slot(0));
        assert!(!tasks.is_running(&slot(0)));
        assert_eq!(tasks.running_count(), 0);

        drop(gate);
        assert!(receiver.recv_timeout(SILENCE_TIMEOUT).is_err());
    }

    #[test]
    fn cancelling_a_slot_that_is_not_running_does_nothing() {
        let (tasks, _receiver) = tasks();
        let (_gate, task) = gated_task();
        tasks.submit(slot(0), move |_cancel| task());

        tasks.cancel(&slot(1));
        assert!(tasks.is_running(&slot(0)));
    }

    #[test]
    fn delivers_the_error_of_a_failing_task() {
        let (tasks, receiver) = tasks();

        tasks.submit(slot(0), |_cancel| {
            Err(CommandError::Status("no such revision".to_owned(), Some(1)).into())
        });

        let output = next_result(&receiver).output;
        assert!(output.unwrap_err().to_string().contains("no such revision"));
    }

    #[test]
    fn survives_a_panicking_task() {
        let (tasks, receiver) = tasks();

        tasks.submit(slot(0), |_cancel| panic!("task blew up"));

        let result = next_result(&receiver);
        assert!(matches!(result.output, Err(TaskError::Panic)));
        tasks.finish(&result);
        assert_eq!(tasks.running_count(), 0);
    }

    #[test]
    fn a_result_from_an_evicted_task_leaves_its_replacement_alone() {
        let (tasks, receiver) = tasks();
        // The first run of the slot is done and its result is on the
        // channel, but the main loop has not picked it up yet, so we still
        // have its entry.
        tasks.submit(slot(0), |_cancel| Ok("first".to_owned()));
        let first = next_result(&receiver);

        // Fill the remaining slots, so making room for one more drops that
        // entry.
        let _gates: Vec<_> = (1..MAX_RUNNING)
            .map(|index| {
                let (gate, task) = gated_task();
                tasks.submit(slot(index), move |_cancel| task());
                gate
            })
            .collect();
        tasks.submit(slot(MAX_RUNNING), |_cancel| Ok("newest".to_owned()));
        assert!(
            !tasks.is_running(&slot(0)),
            "the oldest entry should be gone"
        );

        // With no entry left, the slot takes a second run -- which the
        // first run's result must not release.
        let (_gate, second) = gated_task();
        tasks.submit(slot(0), move |_cancel| second());
        tasks.finish(&first);

        assert!(
            tasks.is_running(&slot(0)),
            "the second run should still hold the slot"
        );
    }

    #[test]
    fn keeps_the_newest_task_of_a_burst() {
        let (tasks, _receiver) = tasks();
        let mut gates = fill_with_gated_tasks(&tasks);

        // A burst of submissions, each of which makes room for itself by
        // killing the oldest task.
        for index in MAX_RUNNING..MAX_RUNNING * 2 {
            let (gate, task) = gated_task();
            gates.push(gate);
            tasks.submit(slot(index), move |_cancel| task());
        }

        assert_eq!(tasks.running_count(), MAX_RUNNING);
        assert!(tasks.is_running(&slot(MAX_RUNNING * 2 - 1)));
    }
}
