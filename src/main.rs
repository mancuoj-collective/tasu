use anyhow::{Context, Result};
use crossterm::event;

use crate::app::App;

pub mod app;
pub mod theme;
pub mod todo;
pub mod ui;

fn main() -> Result<()> {
    ratatui::run(|terminal| {
        let mut app = App::new();
        while !app.should_quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            let ev = event::read().context("failed to read crossterm event")?;
            if let Some(key) = ev.as_key_press_event() {
                app.on_key(key);
            }
        }
        Ok(())
    })
}
