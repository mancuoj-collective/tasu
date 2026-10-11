//! End-to-end TUI flows: drive the real key/update path, render through
//! ratatui's `TestBackend`, and assert on the screen and the file.

mod common;
use common::{dt, press, render, type_str};

use crossterm::event::KeyCode;
use ratatui::Terminal;

use tasu::app::{Action, Effect, Mode, Model, Problem, Runtime, update};
use tasu::config::Config;
use tasu::domain::{Board, Bucket, TaskState};
use tasu::store::Store;

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
fn overlay_scroll_moves_back_up_from_the_bottom() {
    let mut model = Model::new(Board::new(), dt(5));
    model.ui.mode = Mode::Help;

    // Scroll to the bottom on a short terminal, rendering after each press so
    // the modal clamps the offset.
    for _ in 0..30 {
        update(&mut model, press(crossterm::event::KeyCode::Down), dt(5));
        render(&mut model, 60, 12);
    }
    let bottom = model.ui.overlay_scroll;
    assert!(bottom > 0, "help should have scrolled");

    update(&mut model, press(crossterm::event::KeyCode::Up), dt(5));
    render(&mut model, 60, 12);
    assert_eq!(model.ui.overlay_scroll, bottom - 1, "up should move back");
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
    // The same hint must survive on a wide (kanban) terminal.
    assert!(render(&mut model, 120, 12).contains("press a to add something"));
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

    // Another writer changes the file. Force a later mtime instead of sleeping:
    // on a coarse filesystem the two writes can share a timestamp, and the test
    // would race the clock rather than the behaviour under test.
    let path = config.board_path();
    let mut other = Board::new();
    other.add("from elsewhere", dt(5));
    Store::new(&path).save(&other).unwrap();
    let bumped =
        std::fs::metadata(&path).unwrap().modified().unwrap() + std::time::Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(bumped))
        .unwrap();

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
fn a_clean_quit_does_not_rewrite_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        remote: None,
    };
    let mut runtime = Runtime::new(&config);
    let mut model = Model::new(runtime.load_board(dt(5)).board, dt(5));

    // Save once, so the board is on disk.
    runtime.apply(&mut model, vec![Effect::Save]);
    let path = config.board_path();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();

    // A clean quit must not touch the file.
    runtime.apply(&mut model, vec![Effect::Quit]);
    let after = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(before, after, "a clean quit must not rewrite the board");
    assert!(runtime.quit_error().is_none());
}

#[test]
fn a_corrupt_board_is_reported_and_its_backup_is_readable() {
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
    model.ui.board_damage = loaded.damaged.as_ref().map(Problem::from_damage);

    let screen = render(&mut model, 80, 12);
    assert!(
        screen.contains("board unreadable"),
        "the footer should explain the missing board:\n{screen}"
    );
    assert!(
        screen.contains("! details"),
        "the footer should point at the details overlay:\n{screen}"
    );

    // The backup path is too long for the one-line footer; it lives in the
    // details overlay, which `!` opens.
    update(&mut model, press(KeyCode::Char('!')), now);
    assert_eq!(model.ui.mode, Mode::Details);
    let screen = render(&mut model, 80, 12);
    assert!(
        screen.contains(".corrupt-"),
        "the details overlay should name the backup:\n{screen}"
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
