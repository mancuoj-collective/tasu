//! End-to-end tests: drive the real key/update path, render through ratatui's
//! `TestBackend`, and assert on the screen, the file and the git remote.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Local, TimeZone};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use tasu::app::{Action, Effect, Model, update};
use tasu::command;
use tasu::config::Config;
use tasu::domain::Board;
use tasu::store::Store;
use tasu::ui;

fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
}

fn press(code: crossterm::event::KeyCode) -> Action {
    Action::Key(crossterm::event::KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    ))
}

fn type_str(model: &mut Model, text: &str, now: DateTime<Local>) {
    for c in text.chars() {
        update(model, press(crossterm::event::KeyCode::Char(c)), now);
    }
}

fn render(model: &Model, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw(
                frame,
                model,
                &ui::theme::Theme::DARK,
                Path::new("/tmp/tasu/todos.json"),
            )
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()));
        }
        out.push('\n');
    }
    out
}

#[test]
fn capture_shows_on_screen_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("todos.json"));
    let now = dt(5);
    let mut model = Model::new(store.load(), now);

    update(&mut model, press(crossterm::event::KeyCode::Char('a')), now);
    type_str(&mut model, "learn gpui", now);
    let effects = update(&mut model, press(crossterm::event::KeyCode::Enter), now);
    assert_eq!(effects, vec![Effect::Save]);
    store.save(&model.board).unwrap();

    let screen = render(&model, 80, 12);
    assert!(screen.contains("TODAY"), "missing TODAY header:\n{screen}");
    assert!(
        screen.contains("learn gpui"),
        "task not rendered:\n{screen}"
    );

    let persisted = std::fs::read_to_string(store.path()).unwrap();
    assert!(persisted.contains("learn gpui"));
}

#[test]
fn completing_removes_from_list_and_fills_completed() {
    let mut board = Board::new();
    board.add("ship it", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(crossterm::event::KeyCode::Char(' ')), now);
    assert!(!render(&model, 80, 12).contains("ship it"));

    update(&mut model, press(crossterm::event::KeyCode::Char('c')), now);
    assert!(render(&model, 80, 12).contains("ship it"));
}

#[test]
fn stale_task_ages_into_the_week_section() {
    let mut board = Board::new();
    board.add("old task", dt(5));
    let mut model = Model::new(board, dt(5));

    update(&mut model, Action::Tick, dt(6));

    let screen = render(&model, 80, 14);
    let today = screen.find("TODAY").unwrap();
    let week = screen.find("WEEK").unwrap();
    let later = screen.find("LATER").unwrap();
    let task = screen.find("old task").unwrap();
    assert!(
        today < week && week < task && task < later,
        "wrong section:\n{screen}"
    );
}

#[test]
fn wide_terminal_uses_three_columns() {
    let mut board = Board::new();
    board.add("one", dt(5));
    let model = Model::new(board, dt(5));

    let screen = render(&model, 120, 14);
    let header_row = screen
        .lines()
        .find(|line| line.contains("TODAY"))
        .expect("no header row");
    assert!(header_row.contains("TODAY"));
    assert!(header_row.contains("WEEK"));
    assert!(header_row.contains("LATER"));
}

#[test]
fn cli_add_syncs_to_a_remote_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    std::fs::create_dir_all(&first).unwrap();

    let remote = dir.path().join("remote.git");
    let status = Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .arg(&remote)
        .status()
        .unwrap();
    assert!(status.success());

    let config = Config {
        data_dir: first,
        remote: Some(remote.to_string_lossy().into_owned()),
    };
    command::add(&config, &["Nand2Tetris".to_string()]).unwrap();

    let second = dir.path().join("second");
    clone(&remote, &second);
    let board = Store::new(second.join("todos.json")).load();
    assert!(
        board.tasks().iter().any(|task| task.title == "Nand2Tetris"),
        "remote board missing the task"
    );
}

fn clone(remote: &Path, into: &Path) {
    let status = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(remote)
        .arg(into)
        .status()
        .unwrap();
    assert!(status.success(), "clone failed");
}
