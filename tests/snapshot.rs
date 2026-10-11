//! Snapshot tests for the rendered layout, so visual changes show up in review.

use std::path::Path;

use chrono::{DateTime, Local};

mod common;
use common::{draw_text, dt};

use tasu::app::{Mode, Model, SyncStatus};
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
    Board::from_tasks(vec![
        task("Nand2Tetris chapter 6", Bucket::Today, dt(8)),
        task("learn GPUI events", Bucket::Today, dt(8)),
        task("write the README", Bucket::Week, dt(6)),
        task("read the ratatui source", Bucket::Later, dt(2)),
    ])
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
    model.ui.sync = SyncStatus::Failed("remote not found");
    insta::assert_snapshot!(render(&model, 84, 26));
}

#[test]
fn help_hides_the_sync_error_and_the_footer_carries_it() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Help;
    model.ui.sync = SyncStatus::Failed("auth failed");
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
        "raw git error leaked into the UI:\n{out}"
    );
    assert!(
        out.contains("auth failed") && out.contains("run tasu sync"),
        "the footer should explain the failure:\n{out}"
    );
    insta::assert_snapshot!(out);
}
