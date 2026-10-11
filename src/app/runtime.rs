//! The I/O half of the app: owns persistence and the sync worker, and carries
//! out the sync decisions made by the [`SyncTracker`]. Everything that touches
//! the disk, the clock or the network lives here, so [`update`](super::update)
//! stays pure.

use std::time::{Instant, SystemTime};

use chrono::{DateTime, Local};

use crate::config::Config;
use crate::domain::settle;
use crate::store::{Loaded, Store};
use crate::sync::Sync;

use super::action::{Action, Effect};
use super::model::{Model, Problem, SyncStatus};
use super::tracker::{Due, SyncTracker};

/// Owns the store and the sync worker, plus the mtime baseline that separates
/// our own writes from external ones.
pub struct Runtime {
    store: Store,
    sync: Option<Sync>,
    tracker: SyncTracker,
    last_mtime: Option<SystemTime>,
    /// A save that failed while quitting. The terminal is already leaving the
    /// alternate screen by then, so the caller reports it after the UI closes.
    quit_error: Option<String>,
}

impl Runtime {
    pub fn new(config: &Config) -> Self {
        let sync = config
            .remote
            .as_ref()
            .map(|remote| Sync::new(config.data_dir.clone(), Some(remote.clone())));
        Self {
            store: Store::new(config.board_path()),
            tracker: SyncTracker::new(sync.is_some(), Instant::now()),
            sync,
            last_mtime: None,
            quit_error: None,
        }
    }

    /// Footer sync status, copied into the model each frame.
    pub fn status(&self) -> SyncStatus {
        self.tracker.status()
    }

    /// Whether the board has changes the worker has not confirmed pushing, so
    /// quitting should hand a final push to a detached process. A clean quit
    /// spawns nothing.
    pub fn needs_flush(&self) -> bool {
        self.tracker.needs_flush()
    }

    /// The error from a save that failed on quit, if any.
    pub fn quit_error(&self) -> Option<&str> {
        self.quit_error.as_deref()
    }

    /// Load the board, settle it to `now`, persist it if it aged, and refresh
    /// the mtime baseline. Used once, before the loop starts.
    pub fn load_board(&mut self, now: DateTime<Local>) -> Loaded {
        let loaded = self.store.load();
        let mut board = loaded.board;
        if settle(&mut board, now)
            && let Err(err) = self.store.save(&board)
        {
            eprintln!("tasu: could not save the board: {err}");
        }
        self.sync_mtime();
        Loaded {
            board,
            damaged: loaded.damaged,
        }
    }

    /// Kick off the startup pull, marking sync as in progress.
    pub fn start_sync(&mut self) {
        if let Some(sync) = &self.sync {
            match sync.pull() {
                Ok(()) => self.tracker.pull_queued(),
                Err(err) => self.tracker.failed(&err),
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
                            model.ui.board_damage = None;
                            // Only a save that actually reached disk may be
                            // pushed; otherwise we would publish a stale board.
                            self.tracker.changed(Instant::now());
                        }
                        Err(err) => {
                            model.ui.error = Some(Problem::new(
                                "could not save",
                                format!("could not save: {err}"),
                            ))
                        }
                    }
                    self.sync_mtime();
                }
                Effect::Quit => {
                    model.should_quit = true;
                    if let Err(err) = self.store.save(&model.board) {
                        self.quit_error = Some(format!("could not save the board: {err}"));
                    }
                }
            }
        }
    }

    /// Collect finished sync work, carry out the scheduled push, and detect an
    /// external change to the board file. Returns the actions for `update` to
    /// apply, in order.
    pub fn poll_external(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(sync) = &self.sync {
            // The repository could not be adopted or created: surface it rather
            // than letting the app look healthy while nothing actually syncs.
            if let Some(Err(err)) = sync.poll_init() {
                self.tracker.failed(&err);
            }

            let now = Instant::now();
            while let Some(outcome) = sync.poll() {
                // Hand the remote board to the model; it merges and saves, so the
                // worker never writes the board and cannot race a concurrent save.
                if let Some(board) = self.tracker.on_outcome(outcome, now) {
                    actions.push(Action::Merge(board));
                }
            }

            match self.tracker.due(now) {
                Due::Nothing => {}
                due => dispatch(&mut self.tracker, sync, due),
            }
        }

        if let Some(action) = self.reload_if_changed() {
            actions.push(action);
        }
        actions
    }

    /// A reload action when the board file changed underneath us. Our own writes
    /// refresh the baseline in [`Runtime::apply`], so they never come back here.
    fn reload_if_changed(&mut self) -> Option<Action> {
        let current = self.store.mtime();
        if current == self.last_mtime || current.is_none() {
            return None;
        }
        self.last_mtime = current;
        let loaded = self.store.load();
        Some(Action::Reload {
            board: loaded.board,
            damaged: loaded.damaged.map(|damage| damage.detail()),
        })
    }
}

/// Hand the worker the job the tracker asked for. After a failure the pull goes
/// first, so a rejected non-fast-forward push can recover instead of retrying
/// forever.
fn dispatch(tracker: &mut SyncTracker, sync: &Sync, due: Due) {
    if due == Due::PullThenPush {
        if let Err(err) = sync.pull() {
            tracker.failed(&err);
            return;
        }
        tracker.pull_queued();
    }
    match sync.commit_push() {
        Ok(()) => tracker.push_queued(),
        Err(err) => tracker.failed(&err),
    }
}
