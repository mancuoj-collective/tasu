//! End-to-end CLI flows: drive the real built binary and assert on exit codes,
//! stderr and the persisted board.

mod common;
use common::{bare_remote, tasu};

use tasu::domain::Bucket;
use tasu::store::Store;

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
fn cli_move_to_the_same_bucket_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    let out = tasu(&data, &cfg).args(["add", "stay"]).output().unwrap();
    assert!(out.status.success());

    // It is already in today; a move there must not claim to have moved it.
    let out = tasu(&data, &cfg)
        .args(["move", "today", "stay"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a no-op move must be refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already in"), "{stderr}");
}

#[test]
fn cli_done_completes_a_task() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    assert!(
        tasu(&data, &cfg)
            .args(["add", "finish me"])
            .status()
            .unwrap()
            .success()
    );
    let out = tasu(&data, &cfg)
        .args(["done", "finish me"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let board = Store::new(data.join("todos.json")).load().board;
    assert_eq!(board.done().len(), 1, "the task must be done on disk");
    assert!(board.open_in(Bucket::Today).is_empty());
}

#[test]
fn cli_history_reports_done_and_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    for title in ["kept", "dropped"] {
        assert!(
            tasu(&data, &cfg)
                .args(["add", title])
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        tasu(&data, &cfg)
            .args(["done", "kept"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        tasu(&data, &cfg)
            .args(["drop", "dropped"])
            .status()
            .unwrap()
            .success()
    );

    let out = tasu(&data, &cfg)
        .args(["history", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let tasks: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        tasks.as_array().unwrap().len(),
        2,
        "done and dropped are both listed: {tasks}"
    );
}

#[test]
fn cli_config_reports_the_resolved_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");

    let out = tasu(&data, &cfg)
        .args(["config", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data_dir"], data.display().to_string());
    assert!(value["remote"].is_null(), "no remote is configured");
}

#[test]
fn cli_remote_probe_failure_clears_the_previous_remote() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let cfg = dir.path().join("cfg");
    let remote = bare_remote(dir.path());
    let url = remote.to_string_lossy().into_owned();

    let out = tasu(&data, &cfg).args(["remote", &url]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A definitively missing repository is refused and the previous remote is
    // cleared, not left to be retried forever.
    let bad = dir.path().join("nope.git");
    let out = tasu(&data, &cfg)
        .args(["remote", bad.to_string_lossy().as_ref()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a missing remote must be refused");

    let out = tasu(&data, &cfg)
        .args(["config", "--json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["remote"].is_null(),
        "the previous remote must be cleared: {value}"
    );
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
