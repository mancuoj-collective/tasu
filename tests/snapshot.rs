//! Snapshot tests for the rendered layout, so visual changes show up in review.

use std::path::Path;

use chrono::{DateTime, Local};

mod common;
use common::{draw_text, dt};

use tasu::app::{Mode, Model, Problem, SyncStatus};
use tasu::domain::{Board, Bucket, Task, TaskState};

fn render(model: &Model, width: u16, height: u16) -> String {
    draw_text(
        model,
        width,
        height,
        Path::new("/tmp/tasu/todos.json"),
        Some("git@github.com:you/tasu-data.git"),
    )
    .0
}

fn render_at(model: &Model, width: u16, height: u16, path: &Path, sync: Option<&str>) -> String {
    draw_text(model, width, height, path, sync).0
}

fn task(title: &str, bucket: Bucket, since: DateTime<Local>) -> Task {
    Task {
        title: title.to_string(),
        state: TaskState::Open,
        bucket,
        bucket_since: since,
        created_at: since,
        completed_at: None,
        archived_at: None,
        updated_at: None,
    }
}

fn board() -> Board {
    let mut tasks = vec![
        task("Nand2Tetris chapter 6", Bucket::Today, dt(8)),
        task("learn GPUI events", Bucket::Today, dt(8)),
        task("write the README", Bucket::Week, dt(6)),
        task("read the ratatui source", Bucket::Later, dt(2)),
    ];
    // `created_at` is the task identity, so the two Today tasks need distinct
    // instants even though they share a bucket; only the newest-first order
    // matters to the render.
    tasks[1].created_at -= chrono::TimeDelta::nanoseconds(1);
    Board::from_tasks(tasks)
}

#[test]
fn narrow_layout() {
    let model = Model::new(board(), dt(8));
    insta::assert_snapshot!(render(&model, 84, 20));
}

#[test]
fn wide_layout() {
    let model = Model::new(board(), dt(8));
    insta::assert_snapshot!(render(&model, 120, 20));
}

#[test]
fn help_with_sync_error() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Help;
    model.ui.sync_problem = Some(Problem::new(
        "remote not found",
        "remote: Repository not found.\nfatal: repository 'you/tasu-data' does not exist",
    ));
    insta::assert_snapshot!(render(&model, 84, 26));
}

#[test]
fn help_hides_the_sync_error_and_the_footer_carries_it() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Help;
    model.ui.sync = SyncStatus::Idle;
    model.ui.sync_problem = Some(Problem::new(
        "auth failed",
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
    ));
    // The path stays outside any home directory: `display_path` abbreviates the
    // home dir, which differs between machines and would make this snapshot
    // environment-dependent.
    let out = render_at(
        &model,
        84,
        30,
        Path::new("/opt/tasu-data/Library/Application Support/tasu/todos.json"),
        Some("https://github.com/mancuoj-collective/tasu-data.git"),
    );
    assert!(
        !out.contains("fatal:"),
        "the one-line footer must not carry the raw git error:\n{out}"
    );
    assert!(
        out.contains("auth failed") && out.contains("! details"),
        "the footer should summarise the failure and point at the details:\n{out}"
    );
    insta::assert_snapshot!(out);
}

#[test]
fn the_details_overlay_shows_every_problem_in_full() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Details;
    model.ui.error = Some(Problem::new(
        "could not save",
        "failed to write temporary file: Permission denied (os error 13)",
    ));
    model.ui.sync_problem = Some(
        Problem::new(
            "offline",
            "fatal: unable to access: Could not resolve host: github.com",
        )
        .with_hint("run `tasu sync` to retry now"),
    );
    insta::assert_snapshot!(render(&model, 84, 26));
}

#[test]
fn empty_board() {
    let model = Model::new(Board::new(), dt(8));
    insta::assert_snapshot!(render(&model, 84, 12));
}

#[test]
fn too_small_terminal() {
    let model = Model::new(board(), dt(8));
    insta::assert_snapshot!(render(&model, 30, 6));
}

#[test]
fn capture_toast() {
    let mut model = Model::new(board(), dt(8));
    model.set_toast("saved to today");
    insta::assert_snapshot!(render(&model, 84, 20));
}

#[test]
fn add_modal() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Add;
    model.ui.input = tui_input::Input::new("write a snapshot".to_string());
    insta::assert_snapshot!(render(&model, 84, 20));
}

#[test]
fn history_modal() {
    let mut board = board();
    board.complete(0, dt(9));
    board.archive(1, dt(9));
    let mut model = Model::new(board, dt(9));
    model.ui.mode = Mode::Completed;
    insta::assert_snapshot!(render(&model, 84, 20));
}
