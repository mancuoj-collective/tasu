#![forbid(unsafe_code)]

use std::time::Duration;

use anyhow::Result;
use chrono::Local;
use clap::Parser;
use crossterm::event;

use tasu::app::{Action, Model, Runtime, update};
use tasu::cli::{Cli, Command, HistoryName};
use tasu::command;
use tasu::config::Config;
use tasu::domain::Bucket;
use tasu::ui::{self, theme::Theme};

/// How often the loop wakes up to settle, expire the toast and poll for
/// external changes.
const TICK: Duration = Duration::from_millis(200);

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
        Some(Command::List { bucket, json }) => {
            command::list(&config, bucket.map(Bucket::from), json)
        }
        Some(Command::History { view, json }) => command::history(
            &config,
            match view {
                None => command::HistoryList::Both,
                Some(HistoryName::Done) => command::HistoryList::Done,
                Some(HistoryName::Dropped) => command::HistoryList::Dropped,
            },
            json,
        ),
        Some(Command::Done { title }) => command::done(&config, &title),
        Some(Command::Drop { title }) => command::drop_task(&config, &title),
        Some(Command::Move { bucket, title }) => command::move_task(&config, bucket.into(), &title),
        Some(Command::Config { json }) => command::config(&config, json),
        Some(Command::Remote { url, clear }) => command::remote(&config, url.as_deref(), clear),
        Some(Command::Sync) => command::sync(&config),
        Some(Command::Update) => command::update(),
        Some(Command::Completions { shell }) => command::completions(shell),
        Some(Command::Flush) => command::flush(&config),
        None => run_tui(config),
    }
}

fn run_tui(config: Config) -> Result<()> {
    let board_path = config.board_path();
    let remote = config.remote.clone();
    let mut runtime = Runtime::new(&config);

    let now = Local::now();
    let board = runtime.load_board(now);
    let mut model = Model::new(board, now);
    let theme = Theme::detect();

    // Fetch remote state in the background; the mtime watcher reloads it.
    runtime.start_sync();

    // Restore the terminal first; the final push must not freeze the UI.
    let result = ratatui::run(|terminal| {
        while !model.should_quit {
            model.ui.sync = runtime.status();
            model.ui.sync_error = runtime.error().map(str::to_string);
            let mut help_scroll = None;
            terminal.draw(|frame| {
                help_scroll = ui::draw(frame, &model, &theme, &board_path, remote.as_deref());
            })?;
            // The renderer hands back the help overlay's clamped offset.
            if let Some(offset) = help_scroll {
                model.ui.help_scroll = offset;
            }

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
    // Quitting must not wait on the network: the board is already saved on disk,
    // so hand any pending commit+push to a detached `tasu flush` and return now.
    if remote.is_some() && runtime.needs_flush() {
        spawn_background_flush();
    }
    result
}

/// Commit and push in a detached process, so quitting returns to the shell at
/// once. The child inherits the environment, so it opens the same data dir.
fn spawn_background_flush() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe)
        .arg("flush")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
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
