//! Fixtures and render helpers shared by the end-to-end and snapshot tests.

use std::path::Path;

use chrono::{DateTime, Local, TimeZone};
use ratatui::{Terminal, backend::TestBackend};

use tasu::app::Model;
use tasu::ui::{self, Chrome, theme::Theme};

/// A fixed local 09:00 on October `day`, 2026, independent of the machine.
pub fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
}

/// Draw one frame at `width`×`height` and return it as text, one line per row,
/// together with the help overlay's clamped scroll offset when help is open.
pub fn draw_text(
    model: &Model,
    width: u16,
    height: u16,
    data_path: &Path,
    remote: Option<&str>,
) -> (String, Option<usize>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let chrome = Chrome { data_path, remote };
    let mut overlay_scroll = None;
    terminal
        .draw(|frame| {
            overlay_scroll = ui::draw(frame, model, &Theme::DARK, &chrome);
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
    (out, overlay_scroll)
}
