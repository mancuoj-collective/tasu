use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use chrono::Local;
use crossterm::event;

use tasu::app::{Action, Effect, Model, update};
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

fn main() -> Result<()> {
    let config = Config::load();
    let mut runtime = Runtime::new(&config);

    let now = Local::now();
    let mut board = runtime.store.load();
    if settle(&mut board, now) {
        let _ = runtime.store.save(&board);
    }
    runtime.sync_mtime();
    let mut model = Model::new(board, now);
    let theme = Theme::detect();

    // Fetch remote state in the background; the mtime watcher reloads it.
    if let Some(sync) = &runtime.sync {
        sync.pull();
    }

    ratatui::run(|terminal| {
        while !model.should_quit {
            terminal.draw(|frame| ui::draw(frame, &model, &theme))?;

            if let Some(action) = runtime.poll_external() {
                let effects = update(&mut model, action, Local::now());
                runtime.apply(&mut model, effects);
            }

            let action = next_action()?;
            let effects = update(&mut model, action, Local::now());
            runtime.apply(&mut model, effects);
        }
        runtime.flush();
        Ok(())
    })
}

/// Owns the store and the sync worker, plus the small amount of state needed to
/// debounce pushes and distinguish our own writes from external ones.
struct Runtime {
    store: Store,
    sync: Option<Sync>,
    last_mtime: Option<SystemTime>,
    dirty: bool,
    last_change: Instant,
    in_flight: bool,
    retry_at: Option<Instant>,
}

impl Runtime {
    fn new(config: &Config) -> Self {
        Self {
            store: Store::new(config.board_path()),
            sync: config
                .remote
                .as_ref()
                .map(|remote| Sync::new(config.data_dir.clone(), Some(remote.clone()))),
            last_mtime: None,
            dirty: false,
            last_change: Instant::now(),
            in_flight: false,
            retry_at: None,
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
                    let _ = self.store.save(&model.board);
                    self.sync_mtime();
                    self.dirty = true;
                    self.last_change = Instant::now();
                }
                Effect::Quit => {
                    model.should_quit = true;
                    let _ = self.store.save(&model.board);
                }
            }
        }
    }

    /// Debounced push and external-change detection. Returns a reload action
    /// when the board file changed underneath us.
    fn poll_external(&mut self) -> Option<Action> {
        if let Some(sync) = &self.sync {
            while let Some(result) = sync.poll() {
                self.in_flight = false;
                if result.is_err() {
                    self.dirty = true;
                    self.retry_at = Some(Instant::now() + RETRY);
                }
            }
            if self.dirty
                && !self.in_flight
                && self.last_change.elapsed() >= DEBOUNCE
                && self.retry_at.is_none_or(|at| Instant::now() >= at)
            {
                sync.commit_push();
                self.in_flight = true;
                self.dirty = false;
                self.retry_at = None;
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

    fn flush(&mut self) {
        if let Some(sync) = &self.sync {
            sync.flush();
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
