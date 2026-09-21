use anyhow::Result;

use crate::app::App;

pub mod app;
pub mod theme;
pub mod todo;
pub mod ui;

fn main() -> Result<()> {
    ratatui::run(|terminal| App::new().run(terminal))
}
