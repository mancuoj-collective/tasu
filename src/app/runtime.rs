//! The I/O half of the app: owns persistence and the sync worker, and decides
//! when to save, push and reload. Everything that touches the disk, the clock
//! or the network lives here, so [`update`](super::update) stays pure.
//!
//! The scheduling decision itself ([`due`]) is a pure function of a few flags
//! and elapsed times, so the subtle part — the debounce, the retry backoff and
//! "pull first after a failure" — is unit-tested without threads or a clock.

use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Local};

use crate::config::Config;
use crate::domain::{Board, settle};
use crate::store::Store;
use crate::sync::Sync;

use super::action::{Action, Effect};
use super::model::{Model, SyncStatus};

/// Quiet period before a change is pushed.
pub const DEBOUNCE: Duration = Duration::from_secs(3);
/// Backoff after a failed sync before retrying.
pub const RETRY: Duration = Duration::from_secs(30);

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

/// Owns the store and the sync worker, plus the small amount of state needed to
/// debounce pushes and distinguish our own writes from external ones.
pub struct Runtime {
    store: Store,
    sync: Option<Sync>,
    last_mtime: Option<SystemTime>,
    dirty: bool,
    last_change: Instant,
    retry_at: Option<Instant>,
    sync_status: SyncStatus,
    /// Last sync error message, summarised in the footer.
    sync_error: Option<String>,
    /// Number of jobs the worker is still running.
    in_flight: usize,
    /// A previous sync failed; pull before the next push to recover a
    /// rejected (non-fast-forward) push.
    needs_pull: bool,
}

impl Runtime {
    pub fn new(config: &Config) -> Self {
        let sync = config
            .remote
            .as_ref()
            .map(|remote| Sync::new(config.data_dir.clone(), Some(remote.clone())));
        let sync_status = if sync.is_some() {
            SyncStatus::Idle
        } else {
            SyncStatus::Local
        };
        Self {
            store: Store::new(config.board_path()),
            sync,
            last_mtime: None,
            dirty: false,
            last_change: Instant::now(),
            retry_at: None,
            sync_status,
            sync_error: None,
            in_flight: 0,
            needs_pull: false,
        }
    }

    /// Footer sync status, copied into the model each frame.
    pub fn status(&self) -> SyncStatus {
        self.sync_status
    }

    /// Footer error text from the last failed sync, if any.
    pub fn error(&self) -> Option<&str> {
        self.sync_error.as_deref()
    }

    /// Load the board, settle it to `now`, persist it if it aged, and refresh
    /// the mtime baseline. Used once, before the loop starts.
    pub fn load_board(&mut self, now: DateTime<Local>) -> Board {
        let mut board = self.store.load();
        if settle(&mut board, now)
            && let Err(err) = self.store.save(&board)
        {
            eprintln!("tasu: could not save the board: {err}");
        }
        self.sync_mtime();
        board
    }

    /// Kick off the startup pull, marking sync as in progress.
    pub fn start_sync(&mut self) {
        if let Some(sync) = &self.sync {
            match sync.pull() {
                Ok(()) => {
                    self.in_flight += 1;
                    self.sync_status = SyncStatus::Syncing;
                }
                Err(err) => {
                    self.sync_status = SyncStatus::Failed;
                    self.sync_error = Some(err);
                }
            }
        }
    }

    fn sync_mtime(&mut self) {
        self.last_mtime = self.store.mtime();
    }

    /// Persist and schedule sync in response to `update`'s effects.
    pub fn apply(&mut self, model: &mut Model, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Save => {
                    match self.store.save(&model.board) {
                        Ok(()) => {
                            model.ui.error = None;
                            // Only a save that actually reached disk may be
                            // pushed; otherwise we would publish a stale board.
                            self.dirty = true;
                            self.last_change = Instant::now();
                        }
                        Err(err) => model.ui.error = Some(format!("could not save: {err}")),
                    }
                    self.sync_mtime();
                }
                Effect::Quit => {
                    model.should_quit = true;
                    if let Err(err) = self.store.save(&model.board) {
                        eprintln!("tasu: could not save the board: {err}");
                    }
                }
            }
        }
    }

    /// Debounced push and external-change detection. Returns a reload action
    /// when the board file changed underneath us.
    pub fn poll_external(&mut self) -> Option<Action> {
        if let Some(sync) = &self.sync {
            // The repository could not be adopted or created: surface it rather
            // than letting the app look healthy while nothing actually syncs.
            if let Some(Err(err)) = sync.poll_init() {
                self.sync_status = SyncStatus::Failed;
                self.sync_error = Some(err);
            }

            while let Some(result) = sync.poll() {
                self.in_flight = self.in_flight.saturating_sub(1);
                match result {
                    Ok(()) => {
                        self.sync_status = SyncStatus::Idle;
                        self.sync_error = None;
                    }
                    Err(err) => {
                        self.dirty = true;
                        self.needs_pull = true;
                        self.retry_at = Some(Instant::now() + RETRY);
                        self.sync_status = SyncStatus::Failed;
                        self.sync_error = Some(err);
                    }
                }
            }

            let state = SyncState {
                dirty: self.dirty,
                in_flight: self.in_flight,
                since_change: self.last_change.elapsed(),
                retry_pending: self.retry_at.is_some_and(|at| Instant::now() < at),
                needs_pull: self.needs_pull,
            };
            if due(&state, DEBOUNCE) != Due::Nothing {
                // After a failure, pull first so a rejected non-fast-forward
                // push can recover instead of retrying forever.
                if self.needs_pull {
                    if sync.pull().is_ok() {
                        self.in_flight += 1;
                    }
                    self.needs_pull = false;
                }
                if sync.commit_push().is_ok() {
                    self.in_flight += 1;
                    self.dirty = false;
                    self.retry_at = None;
                    self.sync_status = SyncStatus::Syncing;
                } else {
                    self.sync_status = SyncStatus::Failed;
                    self.sync_error = Some("sync worker stopped".to_string());
                }
            }
        }

        let current = self.store.mtime();
        if current != self.last_mtime && current.is_some() {
            self.last_mtime = current;
            let board = self.store.load();
            return Some(Action::Reload(board));
        }
        None
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

    const DEBOUNCE: Duration = Duration::from_secs(3);

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
}
