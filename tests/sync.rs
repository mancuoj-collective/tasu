//! End-to-end sync: two machines against a real git remote, and the
//! single-writer merge.

mod common;
use common::{bare_remote, clone, commit_all, dt, git, press, type_str};

use crossterm::event::KeyCode;

use tasu::app::{Model, Runtime, update};
use tasu::command;
use tasu::config::Config;
use tasu::domain::Board;
use tasu::store::Store;
use tasu::sync;

#[test]
fn cli_add_syncs_to_a_remote_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    std::fs::create_dir_all(&first).unwrap();

    let remote = bare_remote(dir.path());

    let config = Config {
        data_dir: first,
        remote: Some(remote.to_string_lossy().into_owned()),
    };
    command::add(&config, &["Nand2Tetris".to_string()]).unwrap();

    let second = dir.path().join("second");
    clone(&remote, &second);
    let board = Store::new(second.join("todos.json")).load().board;
    assert!(
        board.tasks().iter().any(|task| task.title == "Nand2Tetris"),
        "remote board missing the task"
    );
}

#[test]
fn cli_add_converges_across_two_machines() {
    let dir = tempfile::tempdir().unwrap();
    let remote = bare_remote(dir.path());
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

#[test]
fn a_pull_in_flight_does_not_lose_a_task_added_meanwhile() {
    // Regression for the pull/clobber race: the sync worker used to write
    // `todos.json` itself while the app was saving it. Now the worker hands the
    // remote board back and the app merges and saves it, so a task captured
    // while a pull was in flight must survive.
    let dir = tempfile::tempdir().unwrap();
    let remote = bare_remote(dir.path());
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

    // Drive the loop until the pull's board has been merged in. The bound is
    // generous on purpose: it must not race the worker's own network timeouts,
    // or a slow CI machine reports "never landed" for a pull that is merely
    // still in flight.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
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

// --- sync integration ------------------------------------------------------

#[test]
fn a_later_rename_survives_a_diverged_sync() {
    let dir = tempfile::tempdir().unwrap();
    let remote = bare_remote(dir.path());
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
    let _ = sync::pull_now(&b, Some(&url)).unwrap();
    let mut board_b = Store::new(b.join("todos.json")).load().board;
    assert_eq!(board_b.tasks().len(), 1, "B did not receive the task");

    // A renames it, then B renames it later; the later rename must win.
    board_a.rename(0, "named on A", dt(6));
    Store::new(a.join("todos.json")).save(&board_a).unwrap();
    sync::commit_now(&a, Some(&url)).unwrap();

    board_b.rename(0, "named on B", dt(7));
    Store::new(b.join("todos.json")).save(&board_b).unwrap();
    let _ = sync::pull_now(&b, Some(&url)).unwrap();
    sync::commit_now(&b, Some(&url)).unwrap();

    let merged = Store::new(b.join("todos.json")).load().board;
    assert_eq!(
        merged.task(0).unwrap().title,
        "named on B",
        "the later rename must survive the diverged merge"
    );
}
