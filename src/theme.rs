use ratatui::style::{Color, Style};

pub const BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
pub const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
pub const COMMENT: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
pub const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const SELECTION: Color = Color::Rgb(0x29, 0x2e, 0x42);

pub fn text() -> Style {
    Style::new().fg(FG)
}

pub fn title() -> Style {
    Style::new().fg(BG).bg(BLUE).bold()
}

pub fn key() -> Style {
    Style::new().fg(CYAN)
}

pub fn muted() -> Style {
    Style::new().fg(COMMENT)
}

pub fn selected() -> Style {
    Style::new().fg(FG).bg(SELECTION)
}

pub fn border() -> Style {
    Style::new().fg(BLUE)
}

pub fn danger() -> Style {
    Style::new().fg(RED)
}
