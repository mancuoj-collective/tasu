use anyhow::Result;

use crate::app::App;

pub mod app;
pub mod event;
pub mod ui;

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    result
}
