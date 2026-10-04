//! Render sample boards through `TestBackend` and print them, so the layout can
//! be eyeballed without a real terminal: `cargo run --example preview`.

use chrono::{DateTime, Local, TimeZone};
use ratatui::{Terminal, backend::TestBackend};
use tasu::app::Model;
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

fn main() {
    let now = dt(8);
    let mut tasks = vec![
        task("Nand2Tetris ch6", Bucket::Today, dt(8)),
        task("learn GPUI events", Bucket::Today, dt(8)),
        task("refactor tasu store", Bucket::Week, dt(4)),
        task("write the README", Bucket::Week, dt(6)),
    ];
    for day in 1..=7 {
        tasks.push(task(&format!("someday idea {day}"), Bucket::Later, dt(day)));
    }
    let model = Model::new(Board::from_tasks(tasks), now);

    println!("\n=== narrow (84x20) ===");
    print(&model, 84, 20);
    println!("\n=== wide (120x20) ===");
    print(&model, 120, 20);
    println!("\n=== tiny (60x14) ===");
    print(&model, 60, 14);

    let mut help = model;
    help.ui.mode = tasu::app::Mode::Help;
    println!("\n=== help (84x24) ===");
    print(&help, 84, 24);
}

fn print(model: &Model, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw(
                frame,
                model,
                &ui::theme::Theme::DARK,
                std::path::Path::new("/tmp/tasu/todos.json"),
            )
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    println!("┌{}┐", "─".repeat(width as usize));
    for y in area.top()..area.bottom() {
        let mut row = String::new();
        for x in area.left()..area.right() {
            row.push_str(buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()));
        }
        println!("│{row}│");
    }
    println!("└{}┘", "─".repeat(width as usize));
}
