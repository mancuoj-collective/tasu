//! The interactive loop. Each pass draws the model, collects finished sync work
//! and any external change, then applies one input. The order lives here, not in
//! the binary, so a test can drive the whole loop with scripted input.

use anyhow::Result;
use chrono::Local;
use ratatui::{Terminal, backend::Backend};

use crate::app::{Action, Model, Runtime, update};
use crate::ui::{self, Chrome, theme::Theme};

/// Run until the model asks to quit. `next_action` supplies the next input: a
/// key, or a `Tick` when nothing arrived within the loop's tick.
pub fn run<B>(
    terminal: &mut Terminal<B>,
    runtime: &mut Runtime,
    model: &mut Model,
    theme: &Theme,
    chrome: &Chrome,
    mut next_action: impl FnMut() -> Result<Action>,
) -> Result<()>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    while !model.should_quit {
        model.ui.sync = runtime.status();
        let mut overlay_scroll = None;
        terminal.draw(|frame| {
            overlay_scroll = ui::draw(frame, model, theme, chrome);
        })?;
        // The renderer hands back the help overlay's clamped offset.
        if let Some(offset) = overlay_scroll {
            model.ui.overlay_scroll = offset;
        }

        for action in runtime.poll_external() {
            let effects = update(model, action, Local::now());
            runtime.apply(model, effects);
        }

        let action = next_action()?;
        let effects = update(model, action, Local::now());
        runtime.apply(model, effects);
    }
    Ok(())
}
