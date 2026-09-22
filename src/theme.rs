use ratatui::style::{
    Color::{self, Rgb},
    Style,
};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

#[derive(Clone, Copy)]
pub struct Theme {
    pub fg: Color,
    pub muted: Color,
    pub disabled: Color,
    pub primary: Color,
    pub success: Color,
    pub error: Color,
    pub border: Color,
    pub border_active: Color,
    pub sel_bg: Color,
}

impl Theme {
    pub const DARK: Self = Self {
        fg: Rgb(0xc0, 0xca, 0xf5),
        muted: Rgb(0x56, 0x5f, 0x89),
        disabled: Rgb(0x41, 0x48, 0x68),
        primary: Rgb(0x7a, 0xa2, 0xf7),
        success: Rgb(0x9e, 0xce, 0x6a),
        error: Rgb(0xf7, 0x76, 0x8e),
        border: Rgb(0x3b, 0x42, 0x61),
        border_active: Rgb(0x7a, 0xa2, 0xf7),
        sel_bg: Rgb(0x29, 0x2e, 0x42),
    };

    pub const LIGHT: Self = Self {
        fg: Rgb(0x37, 0x60, 0xbf),
        muted: Rgb(0x84, 0x8c, 0xb5),
        disabled: Rgb(0xa1, 0xa6, 0xc5),
        primary: Rgb(0x2e, 0x7d, 0xe9),
        success: Rgb(0x58, 0x75, 0x39),
        error: Rgb(0xf5, 0x2a, 0x65),
        border: Rgb(0xa8, 0xae, 0xcb),
        border_active: Rgb(0x2e, 0x7d, 0xe9),
        sel_bg: Rgb(0xc4, 0xc8, 0xda),
    };

    pub fn detect() -> Self {
        match theme_mode(QueryOptions::default()) {
            Ok(ThemeMode::Light) => Self::LIGHT,
            _ => Self::DARK,
        }
    }

    pub fn root(&self) -> Style {
        Style::new().fg(self.fg).bg(Color::Reset)
    }

    pub fn text(&self) -> Style {
        Style::new().fg(self.fg)
    }

    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn disabled(&self) -> Style {
        Style::new().fg(self.disabled)
    }

    pub fn success(&self) -> Style {
        Style::new().fg(self.success)
    }

    pub fn border(&self, focused: bool) -> Style {
        let color = if focused {
            self.border_active
        } else {
            self.border
        };
        Style::new().fg(color)
    }

    pub fn title(&self) -> Style {
        Style::new().fg(self.primary).bold()
    }

    pub fn highlight(&self) -> Style {
        Style::new().fg(self.primary).bg(self.sel_bg)
    }

    pub fn key_hint(&self) -> (Style, Style) {
        let key = Style::new().fg(self.primary).bold();
        let label = Style::new().fg(self.muted);
        (key, label)
    }
}
