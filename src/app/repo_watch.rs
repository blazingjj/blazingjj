/*! When the app looks at what the repo is at.

Reading it is a jj invocation, so a check runs as a background task and
its answer arrives later. A [RepoWatch] decides when to start one, what
it is to do, and whether its answer means what the app shows is out of
date and may be caught up without the user asking.
*/

use std::mem;
use std::time::Duration;
use std::time::Instant;

use crate::commander::ids::OperationId;

/// What a check of what the repo is at is to do, and what its answer is
/// to mean.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Check {
    /// Whether to snapshot the working copy while reading, which records
    /// an operation of its own if the working copy has moved on.
    pub snapshot: bool,
    /// Whether a move this check finds is one the app has just made.
    pub ours: bool,
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
    /// How long between polls, or None to only check when asked.
    interval: Option<Duration>,

    /// The operation the last check that answered found.
    op_id: Option<OperationId>,

    /// When a check last started or answered.
    last_check: Instant,

    /// What the next check is to do.
    next: Check,

    /// Whether the next check is not to wait out the interval.
    asked: bool,

    /// What the check the app started last is doing.
    running: Check,

    /// Whether the app itself put the view out of date.
    stale_is_ours: bool,

    /// Whether the terminal window has focus. True to start with, as the
    /// terminal says nothing about focus until it changes.
    has_focus: bool,

    /// Whether the app has left the view out of date for the user to
    /// refresh.
    left_stale: bool,
}

impl RepoWatch {
    /// Polls every `interval`, or only checks when asked if that is None.
    /// Starts out asking to be shown what the repo is at.
    pub fn new(interval: Option<Duration>, now: Instant) -> Self {
        Self {
            interval,
            op_id: None,
            last_check: now,
            next: Check {
                snapshot: true,
                ours: false,
            },
            asked: true,
            running: Check::default(),
            // Nothing has been read yet, and reading it is what the user
            // opened the app for.
            stale_is_ours: true,
            has_focus: true,
            left_stale: false,
        }
    }

    /// Ask for a check that does not wait out the interval.
    pub fn ask_check(&mut self, check: Check) {
        self.asked = true;
        self.want(check);
    }

    /// Note that the app is putting the view out of date itself, without
    /// a check to find it.
    pub fn catching_up(&mut self) {
        self.stale_is_ours = true;
        // A check already on its way was started before we knew that,
        // so what it comes back with says nothing about who moved the
        // repo.
        self.running.ours = true;
    }

    pub fn set_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    /// Whether the view is out of date and the app is waiting to be
    /// asked before catching it up.
    pub fn waiting_for_refresh(&self) -> bool {
        self.left_stale
    }

    /// How long until a poll is due, or None if none is coming and there
    /// is nothing to wait for it on.
    pub fn time_until_poll(&self, moment: Moment) -> Option<Duration> {
        // Having left the view stale, there is nothing more to learn
        // until the user acts on it.
        if self.left_stale || moment.checking || !moment.room {
            return None;
        }

        self.interval.map(|interval| {
            interval.saturating_sub(moment.at.saturating_duration_since(self.last_check))
        })
    }

    /// Decide whether to leave the view out of date rather than catch it
    /// up, and report whether that has changed since the last pass.
    pub fn leave_stale(&mut self, stale: bool) -> bool {
        let left_stale = stale && self.has_focus && !self.stale_is_ours;
        mem::replace(&mut self.left_stale, left_stale) != left_stale
    }

    /// Which check to start now, if any. Never more than one at a time.
    pub fn check_to_start(&mut self, moment: Moment) -> Option<Check> {
        // Even one that was asked for waits its turn.
        if moment.checking || !moment.room {
            return None;
        }
        if !self.asked && !self.is_due(moment) {
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

        if moved && !self.running.ours {
            self.stale_is_ours = false;
        }

        moved
    }

    /// Roll `check` into what the next check is to do, and take a move
    /// it claims as the app's own as one already made.
    fn want(&mut self, check: Check) {
        self.next.snapshot |= check.snapshot;
        self.next.ours |= check.ours;
        if check.ours {
            self.catching_up();
        }
    }

    fn is_due(&self, moment: Moment) -> bool {
        self.time_until_poll(moment)
            .is_some_and(|until| until.is_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_secs(1);

    fn watch(now: Instant) -> RepoWatch {
        RepoWatch::new(Some(INTERVAL), now)
    }

    /// A moment with room for a check and none running.
    fn moment(at: Instant) -> Moment {
        Moment {
            at,
            checking: false,
            room: true,
        }
    }

    fn snapshotting() -> Check {
        Check {
            snapshot: true,
            ours: false,
        }
    }

    fn op_id(id: &str) -> Option<OperationId> {
        Some(OperationId(id.to_owned()))
    }

    #[test]
    fn opens_by_asking_for_a_snapshotting_check() {
        let now = Instant::now();
        let mut watch = watch(now);

        assert_eq!(watch.check_to_start(moment(now)), Some(snapshotting()));
        assert_eq!(watch.check_to_start(moment(now)), None);
    }

    #[test]
    fn polls_once_the_interval_has_passed() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        assert_eq!(watch.check_to_start(moment(now + INTERVAL / 2)), None);
        assert_eq!(
            watch.check_to_start(moment(now + INTERVAL)),
            Some(Check::default())
        );
    }

    #[test]
    fn holds_off_polling_while_a_check_is_running() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));

        let due = now + INTERVAL;
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
        let mut watch = watch(now);
        watch.check_to_start(moment(now));

        let due = now + INTERVAL;
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
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        watch.ask_check(snapshotting());
        assert_eq!(watch.check_to_start(moment(now)), Some(snapshotting()));
    }

    #[test]
    fn keeps_asking_for_a_snapshot_until_one_is_taken() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));

        // A request made while the check runs waits for it rather than
        // taking its answer, which predates the request.
        watch.ask_check(snapshotting());
        assert_eq!(
            watch.check_to_start(Moment {
                checking: true,
                ..moment(now)
            }),
            None
        );

        watch.checked(now, op_id("a"));
        assert_eq!(watch.check_to_start(moment(now)), Some(snapshotting()));
    }

    #[test]
    fn leaves_a_failed_check_to_the_next_one() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));

        // A failure says nothing about the repo, and the snapshot the
        // check was to take is still owed.
        assert!(!watch.checked(now, None));
        assert_eq!(watch.check_to_start(moment(now)), None);
        assert_eq!(
            watch.check_to_start(moment(now + INTERVAL)),
            Some(snapshotting())
        );
    }

    #[test]
    fn reports_the_repo_moving_but_not_the_first_answer() {
        let now = Instant::now();
        let mut watch = watch(now);

        assert!(!watch.checked(now, op_id("a")));
        assert!(!watch.checked(now, op_id("a")));
        assert!(watch.checked(now, op_id("b")));
    }

    #[test]
    fn only_checks_when_asked_without_an_interval() {
        let now = Instant::now();
        let mut watch = RepoWatch::new(None, now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        assert_eq!(watch.time_until_poll(moment(now)), None);
        assert_eq!(watch.check_to_start(moment(now + INTERVAL * 60)), None);

        watch.ask_check(Check::default());
        assert_eq!(watch.check_to_start(moment(now)), Some(Check::default()));
    }

    #[test]
    fn reads_the_view_it_opens_with() {
        let now = Instant::now();
        let mut watch = watch(now);

        // No tab has been read yet, and showing them is what the app was
        // opened for, so none of them is left for the user to ask about.
        assert!(!watch.leave_stale(true));
        assert!(!watch.waiting_for_refresh());
        assert_eq!(watch.time_until_poll(moment(now)), Some(INTERVAL));
    }

    #[test]
    fn has_nothing_to_wake_up_for_while_a_check_cannot_start() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));

        // The answer wakes the loop, so waiting on the clock as well
        // would only spin until it arrives.
        let overdue = now + INTERVAL * 2;
        assert_eq!(
            watch.time_until_poll(Moment {
                checking: true,
                ..moment(overdue)
            }),
            None
        );
        assert_eq!(
            watch.time_until_poll(Moment {
                room: false,
                ..moment(overdue)
            }),
            None
        );

        watch.checked(now, op_id("a"));
        assert_eq!(watch.time_until_poll(moment(now)), Some(INTERVAL));
    }

    #[test]
    fn leaves_a_move_it_found_itself_stale_while_the_window_has_focus() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));
        watch.check_to_start(moment(now + INTERVAL));
        assert!(watch.checked(now, op_id("b")));

        assert!(watch.leave_stale(true));
        assert!(watch.waiting_for_refresh());
        // Nothing more to learn until the user asks.
        assert_eq!(watch.time_until_poll(moment(now)), None);

        // The hint goes as soon as the tab catches up by any route.
        assert!(watch.leave_stale(false));
        assert!(!watch.waiting_for_refresh());
    }

    #[test]
    fn asks_before_catching_up_with_what_changed_while_away() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        // Back at the window, the check that comes with regaining focus
        // reads for a user who is looking at the view again, so what it
        // turns up is theirs to ask for rather than ours to show.
        watch.set_focus(false);
        watch.set_focus(true);
        watch.ask_check(snapshotting());
        watch.check_to_start(moment(now));
        assert!(watch.checked(now, op_id("b")));

        assert!(watch.leave_stale(true));
        assert!(watch.waiting_for_refresh());
    }

    #[test]
    fn catches_up_with_a_move_it_found_itself_while_focus_is_elsewhere() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));
        watch.set_focus(false);
        watch.check_to_start(moment(now + INTERVAL));
        assert!(watch.checked(now, op_id("b")));

        assert!(!watch.leave_stale(true));
        assert!(!watch.waiting_for_refresh());
    }

    #[test]
    fn catches_up_with_a_move_the_app_made_even_with_focus() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        // A command the app ran moved the repo, and the check that finds
        // it answers only after the tab has been read again.
        watch.ask_check(Check {
            snapshot: false,
            ours: true,
        });
        watch.check_to_start(moment(now));
        assert!(!watch.leave_stale(false));

        assert!(watch.checked(now, op_id("b")));
        assert!(!watch.leave_stale(true));
        assert!(!watch.waiting_for_refresh());
    }

    #[test]
    fn stops_treating_staleness_as_its_own_once_a_poll_finds_a_move() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        // Switching tabs is reason enough to catch the view up, but says
        // nothing about the move a later poll turns up.
        watch.catching_up();
        watch.check_to_start(moment(now + INTERVAL));
        assert!(watch.checked(now, op_id("b")));

        assert!(watch.leave_stale(true));
    }

    #[test]
    fn catches_up_with_a_move_a_check_already_running_comes_back_with() {
        let now = Instant::now();
        let mut watch = watch(now);
        watch.check_to_start(moment(now));
        watch.checked(now, op_id("a"));

        // The poll is on its way when the app puts the view behind
        // itself, so it is the one that comes back with the move.
        watch.check_to_start(moment(now + INTERVAL));
        watch.catching_up();
        assert!(watch.checked(now, op_id("b")));

        assert!(!watch.leave_stale(true));
        assert!(!watch.waiting_for_refresh());
    }
}
