//! The sync state machine: what the worker's results mean for the footer, the
//! retry backoff and the next push. Pure — it takes `Instant`s as arguments and
//! never touches the worker, the disk or the clock, so every transition is
//! unit-tested with plain values. [`Runtime`](super::Runtime) performs the I/O.

use std::time::{Duration, Instant};

use crate::domain::Board;
use crate::sync::{Error, JobOutcome};

use super::model::{Problem, SyncStatus};

/// Quiet period before a change is pushed.
const DEBOUNCE: Duration = Duration::from_secs(3);
/// Backoff after a failed sync before retrying.
const RETRY: Duration = Duration::from_secs(30);

/// The flags and elapsed times that decide whether a push is due.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SyncState {
    pub dirty: bool,
    pub in_flight: usize,
    pub since_change: Duration,
    pub retry_pending: bool,
    pub needs_pull: bool,
}

/// What the runtime should do about sync on this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Due {
    Nothing,
    Push,
    /// A previous attempt failed; pull first so a rejected push can recover.
    PullThenPush,
}

/// Pure scheduling: is a push due now, and does it need a pull first? Kept free
/// of `Instant` so it can be tested with plain durations.
pub(crate) fn due(state: &SyncState, debounce: Duration) -> Due {
    if !state.dirty || state.in_flight > 0 || state.since_change < debounce || state.retry_pending {
        return Due::Nothing;
    }
    if state.needs_pull {
        Due::PullThenPush
    } else {
        Due::Push
    }
}

#[derive(Debug)]
pub struct SyncTracker {
    status: SyncStatus,
    /// The last failure, if any, kept whole so the details overlay can show the
    /// git error rather than only the classified phrase.
    problem: Option<Problem>,
    dirty: bool,
    last_change: Instant,
    retry_at: Option<Instant>,
    /// Number of jobs the worker is still running.
    in_flight: usize,
    /// A previous sync failed; pull before the next push to recover a rejected
    /// (non-fast-forward) push.
    needs_pull: bool,
}

impl SyncTracker {
    pub fn new(enabled: bool, now: Instant) -> Self {
        Self {
            status: if enabled {
                SyncStatus::Idle
            } else {
                SyncStatus::Local
            },
            problem: None,
            dirty: false,
            last_change: now,
            retry_at: None,
            in_flight: 0,
            needs_pull: false,
        }
    }

    pub fn status(&self) -> SyncStatus {
        self.status
    }

    /// The last sync failure, for the footer summary and the details overlay.
    pub fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }

    /// Whether the board has changes the worker has not confirmed pushing, so
    /// quitting should hand a final push to a detached process.
    pub fn needs_flush(&self) -> bool {
        self.dirty || self.in_flight > 0 || self.needs_pull
    }

    /// The board was saved locally and must be pushed once it settles.
    pub fn changed(&mut self, now: Instant) {
        self.dirty = true;
        self.last_change = now;
    }

    /// A pull was handed to the worker.
    pub fn pull_queued(&mut self) {
        self.in_flight += 1;
        self.needs_pull = false;
        self.status = SyncStatus::Syncing;
    }

    /// A commit and push was handed to the worker.
    pub fn push_queued(&mut self) {
        self.in_flight += 1;
        self.dirty = false;
        self.retry_at = None;
        self.status = SyncStatus::Syncing;
    }

    /// A sync step could not run, or the repository could not be set up. Keep
    /// the classified phrase for the footer and the whole error for the details
    /// overlay. Nothing is in flight while the retry is pending, so the status
    /// returns to `Idle`.
    pub fn failed(&mut self, error: &Error) {
        self.status = SyncStatus::Idle;
        self.problem = Some(
            Problem::new(error.kind().footer(), error.detail())
                .with_hint("run `tasu sync` to retry now"),
        );
    }

    /// Apply the result of one worker job. A pulled remote board is returned for
    /// the caller to merge and save.
    pub fn on_outcome(&mut self, outcome: JobOutcome, now: Instant) -> Option<Board> {
        self.in_flight = self.in_flight.saturating_sub(1);
        match outcome {
            JobOutcome::UpToDate | JobOutcome::Pushed => {
                self.status = SyncStatus::Idle;
                self.problem = None;
                None
            }
            JobOutcome::Merged(board) => {
                self.status = SyncStatus::Idle;
                self.problem = None;
                board
            }
            JobOutcome::Failed(error) => {
                self.dirty = true;
                self.needs_pull = true;
                self.retry_at = Some(now + RETRY);
                self.failed(&error);
                None
            }
        }
    }

    pub fn due(&self, now: Instant) -> Due {
        due(
            &SyncState {
                dirty: self.dirty,
                in_flight: self.in_flight,
                since_change: now.saturating_duration_since(self.last_change),
                retry_pending: self.retry_at.is_some_and(|at| now < at),
                needs_pull: self.needs_pull,
            },
            DEBOUNCE,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        dirty: bool,
        in_flight: usize,
        since_secs: u64,
        retry_pending: bool,
        needs_pull: bool,
    ) -> SyncState {
        SyncState {
            dirty,
            in_flight,
            since_change: Duration::from_secs(since_secs),
            retry_pending,
            needs_pull,
        }
    }

    #[test]
    fn a_clean_board_is_never_pushed() {
        assert_eq!(
            due(&state(false, 0, 10, false, false), DEBOUNCE),
            Due::Nothing
        );
    }

    #[test]
    fn a_push_waits_out_the_debounce_window() {
        assert_eq!(
            due(&state(true, 0, 1, false, false), DEBOUNCE),
            Due::Nothing,
            "too soon after the change"
        );
        assert_eq!(
            due(&state(true, 0, 3, false, false), DEBOUNCE),
            Due::Push,
            "at the boundary the push is due"
        );
        assert_eq!(due(&state(true, 0, 9, false, false), DEBOUNCE), Due::Push);
    }

    #[test]
    fn nothing_is_scheduled_while_a_job_is_in_flight() {
        assert_eq!(
            due(&state(true, 1, 10, false, true), DEBOUNCE),
            Due::Nothing
        );
    }

    #[test]
    fn the_retry_backoff_holds_the_push() {
        assert_eq!(
            due(&state(true, 0, 10, true, false), DEBOUNCE),
            Due::Nothing,
            "still inside the backoff window"
        );
    }

    #[test]
    fn after_a_failure_the_push_pulls_first() {
        assert_eq!(
            due(&state(true, 0, 10, false, true), DEBOUNCE),
            Due::PullThenPush
        );
    }

    #[test]
    fn a_failed_job_backs_off_then_pulls_before_pushing() {
        let t0 = Instant::now();
        let mut tracker = SyncTracker::new(true, t0);
        tracker.changed(t0);
        tracker.push_queued();
        let board = tracker.on_outcome(
            JobOutcome::Failed(Error::new(
                "fatal: unable to access: Could not resolve host",
            )),
            t0,
        );

        assert!(board.is_none());
        let problem = tracker.problem().expect("a failed push is shown");
        assert_eq!(problem.summary, "offline");
        assert!(
            problem.detail.contains("Could not resolve host"),
            "the git error is kept whole: {}",
            problem.detail
        );
        assert_eq!(tracker.status(), SyncStatus::Idle);
        assert!(tracker.needs_flush(), "a failed push is still owed");
        assert_eq!(tracker.due(t0 + Duration::from_secs(10)), Due::Nothing);
        assert_eq!(tracker.due(t0 + Duration::from_secs(40)), Due::PullThenPush);
    }

    #[test]
    fn a_pulled_remote_board_is_handed_back_and_the_change_stays_owed() {
        let t0 = Instant::now();
        let mut tracker = SyncTracker::new(true, t0);
        tracker.changed(t0);
        tracker.pull_queued();

        let board = tracker.on_outcome(JobOutcome::Merged(Some(Board::new())), t0);

        assert!(board.is_some());
        assert_eq!(tracker.status(), SyncStatus::Idle);
        assert!(tracker.needs_flush(), "the local change is not pushed yet");
    }

    #[test]
    fn a_push_confirmed_by_the_worker_clears_the_debt() {
        let t0 = Instant::now();
        let mut tracker = SyncTracker::new(true, t0);
        tracker.changed(t0);
        tracker.push_queued();
        assert_eq!(tracker.status(), SyncStatus::Syncing);

        tracker.on_outcome(JobOutcome::Pushed, t0);

        assert_eq!(tracker.status(), SyncStatus::Idle);
        assert!(!tracker.needs_flush());
    }

    #[test]
    fn a_local_board_never_reports_sync_state() {
        let tracker = SyncTracker::new(false, Instant::now());
        assert_eq!(tracker.status(), SyncStatus::Local);
        assert!(!tracker.needs_flush());
    }

    #[test]
    fn a_successful_push_clears_the_problem() {
        let t0 = Instant::now();
        let mut tracker = SyncTracker::new(true, t0);
        tracker.changed(t0);
        tracker.push_queued();
        tracker.on_outcome(
            JobOutcome::Failed(Error::new("fatal: could not resolve host")),
            t0,
        );
        assert!(tracker.problem().is_some());

        tracker.on_outcome(JobOutcome::Pushed, t0);
        assert!(tracker.problem().is_none(), "recovery clears the footer");
    }

    #[test]
    fn a_setup_failure_is_classified_for_the_footer() {
        let mut tracker = SyncTracker::new(true, Instant::now());
        tracker.failed(&Error::new("sync worker stopped"));
        let problem = tracker.problem().expect("the setup failure is shown");
        assert_eq!(problem.summary, "sync failed");
        assert_eq!(
            problem.hint.as_deref(),
            Some("run `tasu sync` to retry now")
        );
    }
}
