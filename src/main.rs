#![forbid(unsafe_code)]

use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use chrono::Local;
use clap::Parser;
use crossterm::event;

use tasu::app::{Action, Effect, Model, SyncStatus, update};
use tasu::cli::{Cli, Command};
use tasu::command;
use tasu::config::Config;
use tasu::domain::settle;
use tasu::store::Store;
use tasu::sync::Sync;
use tasu::ui::{self, theme::Theme};

/// How often the loop wakes up to settle, expire the toast and poll for
/// external changes.
const TICK: Duration = Duration::from_millis(200);
/// Quiet period before a change is pushed.
const DEBOUNCE: Duration = Duration::from_secs(3);
/// Backoff after a failed sync before retrying.
const RETRY: Duration = Duration::from_secs(30);

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            // Print the message as authored (commands format it themselves)
            // rather than anyhow's `Error: ...` wrapper.
            eprintln!("{err:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();
    match cli.command {
        Some(Command::Add { title }) => command::add(&config, &title),
        Some(Command::Config) => {
            command::config(&config);
            Ok(())
        }
        Some(Command::Remote { url, clear }) => command::remote(&config, url.as_deref(), clear),
        Some(Command::Sync) => command::sync(&config),
        Some(Command::Update) => command::update(),
        None => run_tui(config),
    }
}

fn run_tui(config: Config) -> Result<()> {
    let board_path = config.board_path();
    let remote = config.remote.clone();
    let mut runtime = Runtime::new(&config);

    let now = Local::now();
    let mut board = runtime.store.load();
    if settle(&mut board, now)
        && let Err(err) = runtime.store.save(&board)
    {
        eprintln!("tasu: could not save the board: {err}");
    }
    runtime.sync_mtime();
    let mut model = Model::new(board, now);
    let theme = Theme::detect();

    // Fetch remote state in the background; the mtime watcher reloads it.
    runtime.start_sync();

    // Restore the terminal first; the final push must not freeze the UI.
    let result = ratatui::run(|terminal| {
        while !model.should_quit {
            model.ui.sync = runtime.sync_status;
            model.ui.sync_error = runtime.sync_error.clone();
            terminal
                .draw(|frame| ui::draw(frame, &model, &theme, &board_path, remote.as_deref()))?;

            if let Some(action) = runtime.poll_external() {
                let effects = update(&mut model, action, Local::now());
                runtime.apply(&mut model, effects);
            }

            let action = next_action()?;
            let effects = update(&mut model, action, Local::now());
            runtime.apply(&mut model, effects);
        }
        Ok(())
    });
    if let Err(err) = runtime.flush() {
        eprintln!("tasu: final push failed: {err}");
    }
    result
}

/// Owns the store and the sync worker, plus the small amount of state needed to
/// debounce pushes and distinguish our own writes from external ones.
struct Runtime {
    store: Store,
    sync: Option<Sync>,
    last_mtime: Option<SystemTime>,
    dirty: bool,
    last_change: Instant,
    retry_at: Option<Instant>,
    sync_status: SyncStatus,
    /// Last sync error message, surfaced in help.
    sync_error: Option<String>,
    /// Number of jobs the worker is still running.
    in_flight: usize,
    /// A previous sync failed; pull before the next push to recover a
    /// rejected (non-fast-forward) push.
    needs_pull: bool,
}

impl Runtime {
    fn new(config: &Config) -> Self {
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

    /// Kick off the startup pull, marking sync as in progress.
    fn start_sync(&mut self) {
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
    fn apply(&mut self, model: &mut Model, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Save => {
                    match self.store.save(&model.board) {
                        Ok(()) => model.ui.error = None,
                        Err(err) => model.ui.error = Some(format!("could not save: {err}")),
                    }
                    self.sync_mtime();
                    self.dirty = true;
                    self.last_change = Instant::now();
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
    fn poll_external(&mut self) -> Option<Action> {
        if let Some(sync) = &self.sync {
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
            if self.dirty
                && self.in_flight == 0
                && self.last_change.elapsed() >= DEBOUNCE
                && self.retry_at.is_none_or(|at| Instant::now() >= at)
            {
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

    fn flush(&mut self) -> Result<(), String> {
        match &self.sync {
            Some(sync) => sync.flush(),
            None => Ok(()),
        }
    }
}

fn next_action() -> Result<Action> {
    if event::poll(TICK)? {
        let event = event::read()?;
        Ok(match event.as_key_press_event() {
            Some(key) => Action::Key(key),
            None => Action::Tick,
        })
    } else {
        Ok(Action::Tick)
    }
}
