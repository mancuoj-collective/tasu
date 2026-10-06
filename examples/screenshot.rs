//! Render the layout to vector SVG screenshots for the README. No external
//! tooling required:
//!
//!     cargo run --example screenshot
//!
//! Writes `assets/kanban-light.svg` and `assets/kanban-dark.svg`. The two share
//! one geometry, so a `<picture>` element can swap them with
//! `prefers-color-scheme`.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Local, TimeZone};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
};

use tasu::app::Model;
use tasu::domain::{Board, Bucket, Task, TaskState};
use tasu::ui::{self, theme::Theme};

const COLS: u16 = 120;
const ROWS: u16 = 12;

/// Monospace advance for the font stack at `FONT_SIZE`, in px. Chosen so a cell
/// maps to a fixed grid regardless of the viewer's actual font.
const CELL_W: f32 = 9.6;
const LINE_H: f32 = 20.0;
const FONT_SIZE: u16 = 16;
const PAD: f32 = 18.0;
const TITLEBAR: f32 = 32.0;

fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
}

fn open(title: &str, bucket: Bucket, day: u32) -> Task {
    Task {
        title: title.to_string(),
        state: TaskState::Open,
        bucket,
        bucket_since: dt(day),
        created_at: dt(day),
        completed_at: None,
        archived_at: None,
    }
}

fn board() -> Board {
    Board::from_tasks(vec![
        open("Nand2Tetris chapter 6", Bucket::Today, 8),
        open("learn GPUI events", Bucket::Today, 8),
        open("refactor the store", Bucket::Week, 6),
        open("write the README", Bucket::Week, 6),
        open("read the ratatui source", Bucket::Later, 3),
        open("haskell mooc", Bucket::Later, 3),
        open("a tiny RSS reader", Bucket::Later, 3),
    ])
}

fn main() {
    let model = Model::new(board(), dt(8));
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    fs::create_dir_all(&assets).unwrap();

    let variants = [
        ("kanban-light.svg", Theme::LIGHT, "#ffffff", "#f4f5fa"),
        ("kanban-dark.svg", Theme::DARK, "#16161e", "#1d1e2c"),
    ];

    for (name, theme, card, bar) in variants {
        let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
        terminal
            .draw(|frame| {
                ui::draw(
                    frame,
                    &model,
                    &theme,
                    Path::new("~/.local/share/tasu/todos.json"),
                    None,
                );
            })
            .unwrap();

        let svg = render_svg(terminal.backend().buffer(), &theme, card, bar);
        fs::write(assets.join(name), svg).unwrap();
        println!("wrote assets/{name}");
    }
}

/// Turn a styled cell buffer into a self-contained SVG: a rounded window card
/// with a macOS-style title bar, then the grid, one anchored glyph per cell so
/// columns stay aligned in any viewer.
fn render_svg(buffer: &Buffer, theme: &Theme, card: &str, bar: &str) -> String {
    let area = buffer.area();
    let width = PAD * 2.0 + area.width as f32 * CELL_W;
    let height = PAD * 2.0 + TITLEBAR + area.height as f32 * LINE_H;
    let x0 = PAD;
    let y0 = PAD + TITLEBAR;
    let fg_default = rgb(theme.fg, card);

    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace" font-size="{FONT_SIZE}">"#
    );

    // Card, then a title bar whose bottom corners are squared off.
    let _ = writeln!(
        s,
        r##"<rect x="0.5" y="0.5" width="{:.0}" height="{:.0}" rx="12" fill="{card}" stroke="#8888aa33"/>"##,
        width - 1.0,
        height - 1.0
    );
    let _ = writeln!(
        s,
        r#"<rect x="0.5" y="0.5" width="{:.0}" height="{:.0}" rx="12" fill="{bar}"/>"#,
        width - 1.0,
        TITLEBAR
    );
    let _ = writeln!(
        s,
        r#"<rect x="0.5" y="{:.1}" width="{:.0}" height="12" fill="{bar}"/>"#,
        TITLEBAR - 12.0,
        width - 1.0
    );
    for (i, dot) in ["#ff5f57", "#febc2e", "#28c840"].iter().enumerate() {
        let cx = 20.0 + i as f32 * 18.0;
        let _ = writeln!(s, r#"<circle cx="{cx:.0}" cy="16" r="5" fill="{dot}"/>"#);
    }

    // Backgrounds first (the selection bar), then the glyphs on top.
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            if cell.bg == Color::Reset {
                continue;
            }
            let bx = x0 + (x - area.left()) as f32 * CELL_W;
            let by = y0 + (y - area.top()) as f32 * LINE_H;
            let fill = rgb(cell.bg, card);
            let _ = writeln!(
                s,
                r#"<rect x="{bx:.1}" y="{by:.1}" width="{CELL_W:.1}" height="{LINE_H:.1}" fill="{fill}"/>"#
            );
        }
    }

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            let symbol = cell.symbol();
            if symbol == " " {
                continue;
            }
            let fill = rgb(cell.fg, &fg_default);
            let mut attrs = String::new();
            if cell.modifier.contains(Modifier::BOLD) {
                attrs.push_str(r#" font-weight="bold""#);
            }
            if cell.modifier.contains(Modifier::CROSSED_OUT) {
                attrs.push_str(r#" text-decoration="line-through""#);
            }
            let tx = x0 + (x - area.left()) as f32 * CELL_W;
            let ty = y0 + (y - area.top()) as f32 * LINE_H + 15.0;
            let _ = writeln!(
                s,
                r#"<text x="{tx:.1}" y="{ty:.1}" fill="{fill}"{attrs}>{}</text>"#,
                escape(symbol)
            );
        }
    }

    s.push_str("</svg>\n");
    s
}

fn rgb(color: Color, fallback: &str) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => fallback.to_string(),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
