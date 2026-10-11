//! End-to-end tests: drive the real key/update path, render through ratatui's
//! `TestBackend`, and assert on the screen, the file and the git remote.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Local};
use ratatui::Terminal;

mod common;
use common::{draw_text, dt};

use crossterm::event::KeyCode;

use tasu::app::{Action, Effect, Mode, Model, Runtime, update};
use tasu::command;
use tasu::config::Config;
use tasu::domain::{Board, Bucket, TaskState};
use tasu::store::Store;
use tasu::sync;

fn press(code: crossterm::event::KeyCode) -> Action {
    Action::Key(crossterm::event::KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    ))
}

fn render(model: &mut Model, width: u16, height: u16) -> String {
    let (text, help_scroll) = draw_text(
        model,
        width,
        height,
        Path::new("/tmp/tasu/todos.json"),
        None,
    );
    if let Some(offset) = help_scroll {
        model.ui.help_scroll = offset;
    }
    text
}

fn type_str(model: &mut Model, text: &str, now: DateTime<Local>) {
    for c in text.chars() {
        update(model, press(crossterm::event::KeyCode::Char(c)), now);
    }
}

#[test]
fn capture_shows_on_screen_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("todos.json"));
    let now = dt(5);
    let mut model = Model::new(store.load().board, now);

    update(&mut model, press(crossterm::event::KeyCode::Char('a')), now);
    type_str(&mut model, "learn gpui", now);
    let effects = update(&mut model, press(crossterm::event::KeyCode::Enter), now);
    assert_eq!(effects, vec![Effect::Save]);
    store.save(&model.board).unwrap();

    let screen = render(&mut model, 80, 12);
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
    assert!(!render(&mut model, 80, 12).contains("ship it"));

    update(&mut model, press(crossterm::event::KeyCode::Char('c')), now);
    assert!(render(&mut model, 80, 12).contains("ship it"));
}

#[test]
fn stale_task_ages_into_the_week_section() {
    let mut board = Board::new();
    board.add("old task", dt(5));
    let mut model = Model::new(board, dt(5));

    update(&mut model, Action::Tick, dt(6));

    let screen = render(&mut model, 80, 14);
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
    let mut model = Model::new(board, dt(5));

    let screen = render(&mut model, 120, 14);
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

    // A hosting provider points its default branch at the first pushed branch;
    // a local bare repo does not, and a raw `git clone` would check out nothing.
    let status = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .status()
        .unwrap();
    assert!(status.success());

    let second = dir.path().join("second");
    clone(&remote, &second);
    let board = Store::new(second.join("todos.json")).load().board;
    assert!(
        board.tasks().iter().any(|task| task.title == "Nand2Tetris"),
        "remote board missing the task"
    );
}

#[test]
fn help_scroll_moves_back_up_from_the_bottom() {
    let mut model = Model::new(Board::new(), dt(5));
    model.ui.mode = Mode::Help;

    // Scroll to the bottom on a short terminal, rendering after each press so
    // the modal clamps the offset.
    for _ in 0..30 {
        update(&mut model, press(crossterm::event::KeyCode::Down), dt(5));
        render(&mut model, 60, 12);
    }
    let bottom = model.ui.help_scroll;
    assert!(bottom > 0, "help should have scrolled");

    update(&mut model, press(crossterm::event::KeyCode::Up), dt(5));
    render(&mut model, 60, 12);
    assert_eq!(model.ui.help_scroll, bottom - 1, "up should move back");
}

#[test]
fn cli_add_converges_across_two_machines() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    let url = remote.to_string_lossy().into_owned();

    let machine_a = dir.path().join("a");
    let machine_b = dir.path().join("b");
    let config_a = Config {
        data_dir: machine_a.clone(),
        remote: Some(url.clone()),
    };
    let config_b = Config {
        data_dir: machine_b.clone(),
        remote: Some(url.clone()),
    };

    // A publishes, B clones and adds, then A pulls before adding again.
    command::add(&config_a, &["from A".to_string()]).unwrap();
    command::add(&config_b, &["from B".to_string()]).unwrap();
    command::add(&config_a, &["back on A".to_string()]).unwrap();

    let board = Store::new(machine_a.join("todos.json")).load().board;
    let titles: Vec<&str> = board
        .tasks()
        .iter()
        .map(|task| task.title.as_str())
        .collect();
    assert!(titles.contains(&"from A"));
    assert!(titles.contains(&"from B"), "A did not pull B's task");
    assert!(titles.contains(&"back on A"));
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

/// Run git in `repo`, panicking on failure. The commit identity is fixed so the
/// test does not depend on the machine's global git config.
fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

#[test]
fn a_pull_in_flight_does_not_lose_a_task_added_meanwhile() {
    // Regression for the pull/clobber race: the sync worker used to write
    // `todos.json` itself while the app was saving it. Now the worker hands the
    // remote board back and the app merges and saves it, so a task captured
    // while a pull was in flight must survive.
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    let url = remote.to_string_lossy().into_owned();

    // The remote starts with one task on `main`.
    let seed = dir.path().join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    let mut base = Board::new();
    base.add("base", dt(5));
    Store::new(seed.join("todos.json")).save(&base).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    commit_all(&seed, "base");
    git(&seed, &["remote", "add", "origin", &url]);
    git(&seed, &["push", "-q", "origin", "main"]);
    let _ = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .status();

    // Machine A clones and adds a task it never pushes, so its history diverges.
    let a = dir.path().join("a");
    clone(&remote, &a);
    let mut board_a = Store::new(a.join("todos.json")).load().board;
    board_a.add("local only", dt(6));
    Store::new(a.join("todos.json")).save(&board_a).unwrap();
    commit_all(&a, "a");

    // The remote moves ahead with a task from another machine.
    let b = dir.path().join("b");
    clone(&remote, &b);
    let mut board_b = Store::new(b.join("todos.json")).load().board;
    board_b.add("from B", dt(7));
    Store::new(b.join("todos.json")).save(&board_b).unwrap();
    commit_all(&b, "b");
    git(&b, &["push", "-q", "origin", "main"]);

    // A opens the app: the pull starts in the background and a task is captured
    // before it lands. The save and the pull must not clobber each other.
    let config = Config {
        data_dir: a.clone(),
        remote: Some(url),
    };
    let mut runtime = Runtime::new(&config);
    let now = dt(8);
    let mut model = Model::new(runtime.load_board(now).board, now);
    runtime.start_sync();

    update(&mut model, press(KeyCode::Char('a')), now);
    type_str(&mut model, "urgent", now);
    let effects = update(&mut model, press(KeyCode::Enter), now);
    runtime.apply(&mut model, effects);

    // Drive the loop until the pull's board has been merged in.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !model
        .board
        .tasks()
        .iter()
        .any(|task| task.title == "from B")
    {
        for action in runtime.poll_external() {
            let effects = update(&mut model, action, now);
            runtime.apply(&mut model, effects);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pull never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // Nothing was lost, on screen or on disk.
    let titles: Vec<String> = model
        .board
        .tasks()
        .iter()
        .map(|task| task.title.clone())
        .collect();
    for wanted in ["base", "local only", "urgent", "from B"] {
        assert!(
            titles.iter().any(|title| title == wanted),
            "lost {wanted:?}: {titles:?}"
        );
    }
    let disk: Vec<String> = Store::new(a.join("todos.json"))
        .load()
        .board
        .tasks()
        .iter()
        .map(|task| task.title.clone())
        .collect();
    for wanted in ["local only", "urgent", "from B"] {
        assert!(
            disk.iter().any(|title| title == wanted),
            "not on disk {wanted:?}: {disk:?}"
        );
    }
}

// --- TUI key flows ---------------------------------------------------------

#[test]
fn editing_renames_the_task() {
    let mut board = Board::new();
    board.add("old title", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char('e')), now);
    for _ in 0.."old title".chars().count() {
        update(&mut model, press(KeyCode::Backspace), now);
    }
    type_str(&mut model, "new title", now);
    let effects = update(&mut model, press(KeyCode::Enter), now);
    assert_eq!(effects, vec![Effect::Save]);

    let screen = render(&mut model, 80, 12);
    assert!(screen.contains("new title"), "rename not shown:\n{screen}");
    assert!(
        !screen.contains("old title"),
        "old title still shown:\n{screen}"
    );
}

#[test]
fn dropping_moves_a_task_to_the_dropped_history() {
    let mut board = Board::new();
    board.add("abandon me", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char('x')), now);
    assert!(
        !render(&mut model, 80, 12).contains("abandon me"),
        "a dropped task must leave the open list"
    );

    update(&mut model, press(KeyCode::Char('c')), now);
    assert!(
        !render(&mut model, 80, 14).contains("abandon me"),
        "a dropped task must not appear under DONE"
    );
    update(&mut model, press(KeyCode::Tab), now);
    assert!(
        render(&mut model, 80, 14).contains("abandon me"),
        "a dropped task must appear under DROPPED"
    );
}

#[test]
fn history_restores_a_completed_task() {
    let mut board = Board::new();
    board.add("finish me", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char(' ')), now);
    assert_eq!(model.selectable_len(), 0);

    update(&mut model, press(KeyCode::Char('c')), now);
    assert!(render(&mut model, 80, 14).contains("finish me"));

    let effects = update(&mut model, press(KeyCode::Enter), now);
    assert_eq!(effects, vec![Effect::Save]);
    assert_eq!(model.board.task(0).unwrap().state, TaskState::Open);
    assert_eq!(model.selectable_len(), 1);
}

#[test]
fn history_restores_a_dropped_task() {
    let mut board = Board::new();
    board.add("reconsider", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char('x')), now);
    assert_eq!(model.selectable_len(), 0);

    update(&mut model, press(KeyCode::Char('c')), now);
    update(&mut model, press(KeyCode::Tab), now);
    let effects = update(&mut model, press(KeyCode::Enter), now);
    assert_eq!(effects, vec![Effect::Save]);
    assert_eq!(model.board.task(0).unwrap().state, TaskState::Open);
    assert_eq!(model.selectable_len(), 1);
}

#[test]
fn moving_between_buckets_changes_the_section() {
    let mut board = Board::new();
    board.add("move me", dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char(']')), now);
    assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Week);
    let screen = render(&mut model, 80, 16);
    let week = screen.find("WEEK").expect("no THIS WEEK header");
    let task = screen.find("move me").expect("task missing");
    assert!(week < task, "task not under THIS WEEK:\n{screen}");

    update(&mut model, press(KeyCode::Char('t')), now);
    assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Today);
}

#[test]
fn h_and_l_jump_the_cursor_between_buckets() {
    let mut board = Board::new();
    board.add("today one", dt(5));
    board.add("later one", dt(5));
    board.move_bucket(1, 2, dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char('l')), now);
    assert_eq!(model.selected(), Some(1), "l should jump to the Later task");
    update(&mut model, press(KeyCode::Char('h')), now);
    assert_eq!(model.selected(), Some(0), "h should jump back to Today");
}

#[test]
fn history_search_filters_the_list() {
    let mut board = Board::new();
    board.add("learn gpui", dt(5));
    board.add("buy cat food", dt(5));
    board.complete(0, dt(5));
    board.complete(1, dt(5));
    let now = dt(5);
    let mut model = Model::new(board, now);

    update(&mut model, press(KeyCode::Char('c')), now);
    assert!(render(&mut model, 80, 14).contains("learn gpui"));

    type_str(&mut model, "cat", now);
    let screen = render(&mut model, 80, 14);
    assert!(screen.contains("buy cat food"), "{screen}");
    assert!(!screen.contains("learn gpui"), "{screen}");
}

#[test]
fn adding_shows_hints_then_a_toast() {
    let now = dt(5);
    let mut model = Model::new(Board::new(), now);

    update(&mut model, press(KeyCode::Char('a')), now);
    let screen = render(&mut model, 80, 12);
    assert!(
        screen.contains("enter") && screen.contains("cancel"),
        "add mode should show its hints:\n{screen}"
    );

    type_str(&mut model, "buy milk", now);
    update(&mut model, press(KeyCode::Enter), now);
    assert!(
        render(&mut model, 80, 12).contains("saved to today"),
        "capture should leave a toast"
    );
}

#[test]
fn an_empty_board_asks_for_a_first_task() {
    let mut model = Model::new(Board::new(), dt(5));
    assert!(render(&mut model, 80, 12).contains("press a to add something"));
}

#[test]
fn a_tiny_terminal_asks_for_a_resize() {
    let mut model = Model::new(Board::new(), dt(5));
    assert!(render(&mut model, 30, 6).contains("terminal too small"));
}

#[test]
fn the_cursor_stays_visible_when_scrolling() {
    let mut board = Board::new();
    for i in 0..20 {
        board.add(format!("task {i}"), dt(5));
    }
    let now = dt(5);
    let mut model = Model::new(board, now);

    for _ in 0..19 {
        update(&mut model, press(KeyCode::Char('j')), now);
    }
    let selected = model.selected().expect("nothing selected");
    let title = model.board.task(selected).unwrap().title.clone();
    let screen = render(&mut model, 80, 12);
    assert!(
        screen.contains(&title),
        "selected {title:?} scrolled off:\n{screen}"
    );
}

// --- persistence and external reload ---------------------------------------

#[test]
fn an_external_write_is_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        remote: None,
    };
    let mut runtime = Runtime::new(&config);
    let mut model = Model::new(runtime.load_board(dt(5)).board, dt(5));

    // Our own write must not look like an external change.
    runtime.apply(&mut model, vec![Effect::Save]);
    assert!(
        runtime.poll_external().is_empty(),
        "our own write was seen as external"
    );

    // Another writer changes the file; give the mtime a moment to move on.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut other = Board::new();
    other.add("from elsewhere", dt(5));
    Store::new(config.board_path()).save(&other).unwrap();

    match runtime.poll_external().as_slice() {
        [Action::Reload { board, .. }] => assert!(
            board
                .tasks()
                .iter()
                .any(|task| task.title == "from elsewhere"),
            "the reload did not carry the external change"
        ),
        other => panic!("expected a reload, got {other:?}"),
    }
}

#[test]
fn a_corrupt_board_is_reported_in_the_footer() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        remote: None,
    };
    std::fs::write(config.board_path(), "{ not json").unwrap();

    let mut runtime = Runtime::new(&config);
    let now = dt(5);
    let loaded = runtime.load_board(now);
    assert!(loaded.board.is_empty(), "a damaged board loads empty");
    let mut model = Model::new(loaded.board, now);
    model.ui.board_damage = loaded.damaged.map(|damage| damage.message());

    let screen = render(&mut model, 80, 12);
    assert!(
        screen.contains("board unreadable"),
        "the footer should explain the missing board:\n{screen}"
    );
    assert!(
        screen.contains(".corrupt-"),
        "the footer should name the backup:\n{screen}"
    );
}

// --- the real binary, end to end -------------------------------------------

/// Run the built `tasu` binary against an isolated data dir and config home, so
/// the developer's own settings never leak into the test.
fn tasu(data: &Path, config_home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tasu"));
    cmd.env("TASU_DATA", data);
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env_remove("TASU_REMOTE");
    cmd
}

#[test]
fn cli_lists_the_board_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    let out = tasu(&data, &cfg)
        .args(["add", "write tests"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = tasu(&data, &cfg).args(["list", "--json"]).output().unwrap();
    assert!(out.status.success());
    let tasks: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = tasks.as_array().expect("list --json is not a JSON array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "write tests");
    assert_eq!(tasks[0]["state"], "open");
    assert_eq!(tasks[0]["bucket"], "today");
}

#[test]
fn cli_rejects_an_ambiguous_title() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    for _ in 0..2 {
        let out = tasu(&data, &cfg).args(["add", "dup"]).output().unwrap();
        assert!(out.status.success());
    }

    let out = tasu(&data, &cfg).args(["done", "dup"]).output().unwrap();
    assert!(
        !out.status.success(),
        "an ambiguous title must not be guessed"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("disambiguate"), "{stderr}");
}

#[test]
fn cli_sync_exits_nonzero_on_an_unreachable_remote() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");
    let bad = dir.path().join("nope.git");

    let out = tasu(&data, &cfg)
        .env("TASU_REMOTE", bad.to_string_lossy().as_ref())
        .arg("sync")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "sync must fail loudly on an unreachable remote"
    );
}

#[test]
fn cli_add_offline_says_the_pull_failed_and_still_captures() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");
    let bad = dir.path().join("nope.git");

    let out = tasu(&data, &cfg)
        .env("TASU_REMOTE", bad.to_string_lossy().as_ref())
        .args(["add", "offline capture"])
        .output()
        .unwrap();
    assert!(out.status.success(), "a capture must not fail offline");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("tasu: pull failed:"), "{stderr}");
    assert!(
        Store::new(data.join("todos.json"))
            .load()
            .board
            .tasks()
            .iter()
            .any(|task| task.title == "offline capture"),
        "the task must be saved locally"
    );
}

#[test]
fn cli_warns_when_the_board_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("todos.json"), "{ not json").unwrap();

    // The command still runs on an empty board, but says so on stderr.
    let out = tasu(&data, &cfg).args(["list"]).output().unwrap();
    assert!(out.status.success(), "list still succeeds");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("board unreadable"), "{stderr}");
    assert!(stderr.contains(".corrupt-"), "{stderr}");
}

// --- sync integration ------------------------------------------------------

#[test]
fn a_later_rename_survives_a_diverged_sync() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    // Pin the bare repo's default branch so every machine agrees on `main`.
    assert!(
        Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .status()
            .unwrap()
            .success()
    );
    let url = remote.to_string_lossy().into_owned();

    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir_all(&a).unwrap();

    // A publishes one task.
    let mut board_a = Board::new();
    board_a.add("shared", dt(5));
    Store::new(a.join("todos.json")).save(&board_a).unwrap();
    sync::commit_now(&a, Some(&url)).unwrap();

    // B clones it.
    sync::pull_now(&b, Some(&url)).unwrap();
    let mut board_b = Store::new(b.join("todos.json")).load().board;
    assert_eq!(board_b.tasks().len(), 1, "B did not receive the task");

    // A renames it, then B renames it later; the later rename must win.
    board_a.rename(0, "named on A", dt(6));
    Store::new(a.join("todos.json")).save(&board_a).unwrap();
    sync::commit_now(&a, Some(&url)).unwrap();

    board_b.rename(0, "named on B", dt(7));
    Store::new(b.join("todos.json")).save(&board_b).unwrap();
    sync::pull_now(&b, Some(&url)).unwrap();
    sync::commit_now(&b, Some(&url)).unwrap();

    let merged = Store::new(b.join("todos.json")).load().board;
    assert_eq!(
        merged.task(0).unwrap().title,
        "named on B",
        "the later rename must survive the diverged merge"
    );
}

#[test]
fn the_session_loop_captures_a_task_from_scripted_keys() {
    use ratatui::backend::TestBackend;
    use tasu::session;
    use tasu::ui::Chrome;
    use tasu::ui::theme::Theme;

    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        remote: None,
    };
    let mut runtime = Runtime::new(&config);
    let mut model = Model::new(runtime.load_board(dt(5)).board, dt(5));

    let mut script: Vec<Action> = vec![press(KeyCode::Char('a'))];
    script.extend("ship it".chars().map(|c| press(KeyCode::Char(c))));
    script.push(press(KeyCode::Enter));
    script.push(press(KeyCode::Char('q')));
    let mut script = script.into_iter();

    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    let chrome = Chrome {
        data_path: dir.path(),
        remote: None,
    };
    session::run(
        &mut terminal,
        &mut runtime,
        &mut model,
        &Theme::DARK,
        &chrome,
        || Ok(script.next().expect("the script ended before quit")),
    )
    .unwrap();

    assert!(model.should_quit);
    let saved = Store::new(config.board_path()).load().board;
    assert!(saved.tasks().iter().any(|task| task.title == "ship it"));
}
