//! Snapshot tests for the rendered layout, so visual changes show up in review.

use std::path::Path;

use chrono::{DateTime, Local, TimeZone};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use tasu::app::{Mode, Model, SyncStatus};
use tasu::domain::{Board, Bucket, Task, TaskState};
use tasu::ui;

fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
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

fn render(model: &Model, width: u16, height: u16) -> String {
    render_full(
        model,
        width,
        height,
        Path::new("/tmp/tasu/todos.json"),
        Some("git@github.com:you/tasu-data.git"),
    )
}

fn render_full(model: &Model, width: u16, height: u16, path: &Path, sync: Option<&str>) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::draw(frame, model, &ui::theme::Theme::DARK, path, sync))
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
    model.ui.sync = SyncStatus::Failed;
    model.ui.sync_error = Some("fatal: repository 'you/tasu-data' not found".to_string());
    insta::assert_snapshot!(render(&model, 84, 26));
}

#[test]
fn help_wraps_long_error_and_paths() {
    let mut model = Model::new(board(), dt(8));
    model.ui.mode = Mode::Help;
    model.ui.sync = SyncStatus::Failed;
    model.ui.sync_error = Some(
        "fatal: could not read Username for 'https://github.com': terminal prompts \
         disabled; fatal: repository not found"
            .to_string(),
    );
    // Keep the path outside any home directory: `display_path` abbreviates the
    // home dir, which differs between machines and would make this snapshot
    // environment-dependent.
    insta::assert_snapshot!(render_full(
        &model,
        84,
        34,
        Path::new("/opt/tasu-data/Library/Application Support/tasu/todos.json"),
        Some("https://github.com/mancuoj-collective/tasu-data.git"),
    ));
}
