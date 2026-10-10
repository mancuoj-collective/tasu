//! Render sample boards through `TestBackend` and print them, so the layout can
//! be eyeballed without a real terminal: `cargo run --example preview`.

use chrono::{DateTime, Local, TimeZone};
use ratatui::{Terminal, backend::TestBackend};
use tasu::app::{Mode, Model};
use tasu::domain::{Board, Bucket, Task, TaskState};
use tasu::ui;

fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
}

fn open(title: &str, bucket: Bucket, since: DateTime<Local>) -> Task {
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

fn done(title: &str, day: u32) -> Task {
    Task {
        title: title.to_string(),
        state: TaskState::Done,
        bucket: Bucket::Today,
        bucket_since: dt(day),
        created_at: dt(day),
        completed_at: Some(dt(day)),
        archived_at: None,
        updated_at: None,
    }
}

fn board() -> Board {
    let mut tasks = vec![
        open("Nand2Tetris ch6", Bucket::Today, dt(8)),
        open("learn GPUI events", Bucket::Today, dt(8)),
        open("refactor tasu store", Bucket::Week, dt(4)),
        open("write the README", Bucket::Week, dt(6)),
    ];
    for day in 1..=7 {
        tasks.push(open(&format!("someday idea {day}"), Bucket::Later, dt(day)));
    }
    Board::from_tasks(tasks)
}

fn main() {
    let now = dt(8);

    let model = Model::new(board(), now);
    println!("\n=== narrow (84x20) ===");
    print(&model, 84, 20);
    println!("\n=== wide (120x20) ===");
    print(&model, 120, 20);
    println!("\n=== tiny (60x14) ===");
    print(&model, 60, 14);

    let mut help = Model::new(board(), now);
    help.ui.mode = Mode::Help;
    println!("\n=== help (84x26) ===");
    print(&help, 84, 26);
    help.ui.help_scroll = 6;
    println!("\n=== help scrolled (60x12) ===");
    print(&help, 60, 12);

    let mut history = Model::new(
        Board::from_tasks(vec![
            done("shipped the store", 6),
            done("read chapter five", 5),
            dropped("that side project", 3),
            dropped("rewrite in rust", 2),
        ]),
        now,
    );
    history.ui.mode = Mode::Completed;
    println!("\n=== history · done (84x18) ===");
    print(&history, 84, 18);
    history.ui.history_view = tasu::app::HistoryView::Dropped;
    println!("\n=== history · dropped (84x18) ===");
    print(&history, 84, 18);
    history.ui.done_filter = tui_input::Input::new("nomatch".to_string());
    println!("\n=== history · empty (84x18) ===");
    print(&history, 84, 18);

    let mut long = Model::new(board(), now);
    long.board.add(
        "an extremely long task title that will definitely run past the right edge of the pane and keep going",
        now,
    );
    for i in 0..20 {
        long.board.add(format!("backlog item number {i}"), now);
    }
    println!("\n=== long title + long list (84x20) ===");
    print(&long, 84, 20);
    println!("\n=== long title, wide (120x20) ===");
    print(&long, 120, 20);
    println!("\n=== too small (30x6) ===");
    print(&long, 30, 6);

    let mut status = Model::new(board(), now);
    status.ui.sync = tasu::app::SyncStatus::Syncing;
    status.ui.tick = 6;
    println!("\n=== syncing (84x12) ===");
    print(&status, 84, 12);
    status.ui.sync = tasu::app::SyncStatus::Failed;
    println!("\n=== sync failed (84x12) ===");
    print(&status, 84, 12);
}

fn dropped(title: &str, day: u32) -> Task {
    Task {
        title: title.to_string(),
        state: TaskState::Archived,
        bucket: Bucket::Later,
        bucket_since: dt(day),
        created_at: dt(day),
        completed_at: None,
        archived_at: Some(dt(day)),
        updated_at: None,
    }
}

fn print(model: &Model, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let _ = ui::draw(
                frame,
                model,
                &ui::theme::Theme::DARK,
                std::path::Path::new("/Users/mancuoj/Library/Application Support/tasu/todos.json"),
                Some("git@github.com:mancuoj/tasu-data.git"),
            );
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
