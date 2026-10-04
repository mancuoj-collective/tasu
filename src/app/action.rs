use crossterm::event::KeyEvent;

use crate::domain::Board;

/// Everything that can move the app forward. `update` is the single place that
/// turns these into state changes and effects.
#[derive(Debug)]
pub enum Action {
    Key(KeyEvent),
    /// Periodic wake-up: settles the pipeline, expires the toast.
    Tick,
    /// The board changed on disk (external write) and was reloaded.
    Reload(Board),
}

/// Side effects `update` asks the runtime to perform. `update` never does I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Save,
    Quit,
}
