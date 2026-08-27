/*! When the app looks at what the repo is at.

Reading it is a jj invocation, so a check runs as a background task and
its answer arrives later. A [RepoWatch] decides when to start one, what
it is to do, and whether its answer means what the app shows is out of
date.
*/

use std::mem;
use std::time::Duration;
use std::time::Instant;

use crate::commander::ids::OperationId;

/// How long after a check of what the repo is at the next one is due.
const CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// What a check of what the repo is at is to do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Check {
    /// Whether to snapshot the working copy while reading, which records
    /// an operation of its own if the working copy has moved on.
    pub snapshot: bool,
}

/// What the app can say about the moment it is asking about.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub at: Instant,
    /// Whether a check is still running.
    pub checking: bool,
    /// Whether a check can start without killing work already running.
    pub room: bool,
}

/// What the app knows about where the repo is.
pub struct RepoWatch {
    /// The operation the last check that answered found.
    op_id: Option<OperationId>,

    /// When a check last started or answered.
    last_check: Instant,

    /// What the next check is to do.
    next: Check,

    /// Whether the next check is not to wait out [CHECK_INTERVAL].
    asked: bool,

    /// What the check the app started last is doing.
    running: Check,
}

impl RepoWatch {
    /// Starts out asking to be shown what the repo is at.
    pub fn new(now: Instant) -> Self {
        Self {
            op_id: None,
            last_check: now,
            next: Check { snapshot: true },
            asked: true,
            running: Check::default(),
        }
    }

    /// Ask for a check that does not wait out the interval.
    pub fn ask_check(&mut self, check: Check) {
        self.asked = true;
        self.want(check);
    }

    /// Which check to start now, if any. Never more than one at a time.
    pub fn check_to_start(&mut self, moment: Moment) -> Option<Check> {
        if moment.checking || !moment.room {
            return None;
        }
        if !self.asked && !self.is_due(moment.at) {
            return None;
        }

        self.asked = false;
        // Starting one counts as a check, so that a check still running
        // does not make the next one due.
        self.last_check = moment.at;
        self.running = mem::take(&mut self.next);

        Some(self.running)
    }

    /// Take what a check found, or None if it could not read the repo,
    /// and report whether the repo has moved since the last answer.
    pub fn checked(&mut self, at: Instant, op_id: Option<OperationId>) -> bool {
        // Timed from when the check is done, so that a slow one does not
        // leave the app checking back to back.
        self.last_check = at;

        let Some(op_id) = op_id else {
            self.want(self.running);
            return false;
        };

        // The first answer has nothing to have moved from.
        let moved = self.op_id.as_ref().is_some_and(|known| known != &op_id);
        self.op_id = Some(op_id);

        moved
    }

    /// Roll `check` into what the next check is to do.
    fn want(&mut self, check: Check) {
        self.next.snapshot |= check.snapshot;
    }

    fn is_due(&self, at: Instant) -> bool {
        at.saturating_duration_since(self.last_check) >= CHECK_INTERVAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A moment with room for a check and none running.
    fn moment(at: Instant) -> Moment {
        Moment {
            at,
            checking: false,
            room: true,
        }
    }

    fn op_id(id: &str) -> Option<OperationId> {
        Some(OperationId(id.to_owned()))
    }

    #[test]
    fn opens_by_asking_for_a_snapshotting_check() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);

        assert_eq!(
            watch.check_to_start(moment(now)),
            Some(Check { snapshot: true })
        );
        assert_eq!(watch.check_to_start(moment(now)), None);
    }

    #[test]
    fn polls_once_the_interval_has_passed() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        assert_eq!(watch.check_to_start(moment(now + CHECK_INTERVAL / 2)), None);
        assert_eq!(
            watch.check_to_start(moment(now + CHECK_INTERVAL)),
            Some(Check::default())
        );
    }

    #[test]
    fn holds_off_polling_while_a_check_is_running() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));

        let due = now + CHECK_INTERVAL;
        assert_eq!(
            watch.check_to_start(Moment {
                checking: true,
                ..moment(due)
            }),
            None
        );
        assert_eq!(watch.check_to_start(moment(due)), Some(Check::default()));
    }

    #[test]
    fn holds_off_polling_while_the_task_pool_is_full() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));

        let due = now + CHECK_INTERVAL;
        assert_eq!(
            watch.check_to_start(Moment {
                room: false,
                ..moment(due)
            }),
            None
        );
        assert_eq!(watch.check_to_start(moment(due)), Some(Check::default()));
    }

    #[test]
    fn starts_an_asked_check_without_waiting_out_the_interval() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        watch.ask_check(Check { snapshot: true });
        assert_eq!(
            watch.check_to_start(moment(now)),
            Some(Check { snapshot: true })
        );
    }

    #[test]
    fn keeps_asking_for_a_snapshot_until_one_is_taken() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));

        // A request made while the check runs waits for it rather than
        // taking its answer, which predates the request.
        watch.ask_check(Check { snapshot: true });
        assert_eq!(
            watch.check_to_start(Moment {
                checking: true,
                ..moment(now)
            }),
            None
        );

        watch.checked(now, op_id("a"));
        assert_eq!(
            watch.check_to_start(moment(now)),
            Some(Check { snapshot: true })
        );
    }

    #[test]
    fn leaves_a_failed_check_to_the_next_one() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);
        watch.check_to_start(moment(now));

        // A failure says nothing about the repo, and the snapshot the
        // check was to take is still owed.
        assert!(!watch.checked(now, None));
        assert_eq!(watch.check_to_start(moment(now)), None);
        assert_eq!(
            watch.check_to_start(moment(now + CHECK_INTERVAL)),
            Some(Check { snapshot: true })
        );
    }

    #[test]
    fn reports_the_repo_moving_but_not_the_first_answer() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(now);

        assert!(!watch.checked(now, op_id("a")));
        assert!(!watch.checked(now, op_id("a")));
        assert!(watch.checked(now, op_id("b")));
    }
}
