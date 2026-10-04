use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use chrono::Local;
use crossterm::event;

use tasu::app::{Action, Effect, Model, update};
use tasu::domain::settle;
use tasu::store::Store;
use tasu::ui;
use tasu::ui::theme::Theme;

/// How often the loop wakes up to settle the pipeline and expire the toast.
const TICK: Duration = Duration::from_millis(200);

fn main() -> Result<()> {
    let store = Store::new(data_path());

    let now = Local::now();
    let mut board = store.load();
    if settle(&mut board, now) {
        let _ = store.save(&board);
    }
    let mut model = Model::new(board, now);
    let theme = Theme::detect();

    ratatui::run(|terminal| {
        while !model.should_quit {
            terminal.draw(|frame| ui::draw(frame, &model, &theme))?;
            let action = next_action()?;
            for effect in update(&mut model, action, Local::now()) {
                handle(effect, &mut model, &store);
            }
        }
        Ok(())
    })
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

fn handle(effect: Effect, model: &mut Model, store: &Store) {
    match effect {
        Effect::Save => {
            let _ = store.save(&model.board);
        }
        Effect::Quit => {
            model.should_quit = true;
            let _ = store.save(&model.board);
        }
    }
}

fn data_path() -> PathBuf {
    if let Ok(dir) = std::env::var("TASU_DATA") {
        return PathBuf::from(dir).join("todos.json");
    }
    dirs::data_dir()
        .map(|dir| dir.join(env!("CARGO_PKG_NAME")).join("todos.json"))
        .unwrap_or_else(|| PathBuf::from("todos.json"))
}
