use ratatui::style::{
    Color::{self, Rgb},
    Style,
};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub bg_panel: Color,
    pub bg_element: Color,
    pub bg_overlay: Color,

    pub fg: Color,
    pub fg_dark: Color,
    pub fg_muted: Color,
    pub fg_disabled: Color,

    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,

    pub info: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,

    pub border: Color,
    pub border_subtle: Color,
    pub border_active: Color,

    pub sel_fg: Color,
    pub sel_bg: Color,
    pub hover_bg: Color,
}

impl Theme {
    pub const DARK: Self = Self {
        bg: Rgb(0x1a, 0x1b, 0x26),
        bg_panel: Rgb(0x16, 0x16, 0x1e),
        bg_element: Rgb(0x29, 0x2e, 0x42),
        bg_overlay: Rgb(0x0c, 0x0e, 0x14),

        fg: Rgb(0xc0, 0xca, 0xf5),
        fg_dark: Rgb(0xa9, 0xb1, 0xd6),
        fg_muted: Rgb(0x56, 0x5f, 0x89),
        fg_disabled: Rgb(0x41, 0x48, 0x68),

        primary: Rgb(0x7a, 0xa2, 0xf7),
        secondary: Rgb(0xbb, 0x9a, 0xf7),
        tertiary: Rgb(0x7d, 0xcf, 0xff),

        info: Rgb(0x2a, 0xc3, 0xde),
        success: Rgb(0x9e, 0xce, 0x6a),
        warning: Rgb(0xe0, 0xaf, 0x68),
        error: Rgb(0xf7, 0x76, 0x8e),

        border: Rgb(0x3b, 0x42, 0x61),
        border_subtle: Rgb(0x29, 0x2e, 0x42),
        border_active: Rgb(0x7a, 0xa2, 0xf7),

        sel_fg: Rgb(0xc0, 0xca, 0xf5),
        sel_bg: Rgb(0x3d, 0x59, 0xa1),
        hover_bg: Rgb(0x29, 0x2e, 0x42),
    };

    pub const LIGHT: Self = Self {
        bg: Rgb(0xe1, 0xe2, 0xe7),
        bg_panel: Rgb(0xd0, 0xd5, 0xe3),
        bg_element: Rgb(0xc4, 0xc8, 0xda),
        bg_overlay: Rgb(0xc1, 0xc9, 0xdf),

        fg: Rgb(0x37, 0x60, 0xbf),
        fg_dark: Rgb(0x61, 0x72, 0xb0),
        fg_muted: Rgb(0x84, 0x8c, 0xb5),
        fg_disabled: Rgb(0xa1, 0xa6, 0xc5),

        primary: Rgb(0x2e, 0x7d, 0xe9),
        secondary: Rgb(0x98, 0x54, 0xf1),
        tertiary: Rgb(0x00, 0x71, 0x97),

        info: Rgb(0x18, 0x80, 0x92),
        success: Rgb(0x58, 0x75, 0x39),
        warning: Rgb(0x8c, 0x6c, 0x3e),
        error: Rgb(0xf5, 0x2a, 0x65),

        border: Rgb(0xa8, 0xae, 0xcb),
        border_subtle: Rgb(0xc4, 0xc8, 0xda),
        border_active: Rgb(0x2e, 0x7d, 0xe9),

        sel_fg: Rgb(0x37, 0x60, 0xbf),
        sel_bg: Rgb(0x78, 0x90, 0xdd),
        hover_bg: Rgb(0xc4, 0xc8, 0xda),
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
        Style::new().fg(self.fg_muted)
    }

    pub fn disabled(&self) -> Style {
        Style::new().fg(self.fg_disabled)
    }

    pub fn border(&self, focused: bool) -> Style {
        let color = if focused {
            self.border_active
        } else {
            self.border
        };
        Style::new().fg(color)
    }

    pub fn title(&self, focused: bool) -> Style {
        let color = if focused { self.primary } else { self.fg_muted };
        Style::new().fg(color).bold()
    }
    pub fn highlight(&self) -> Style {
        Style::new().fg(self.primary).bg(self.hover_bg)
    }

    pub fn key_hint(&self) -> (Style, Style) {
        let key = Style::new().fg(self.primary).bold();
        let label = Style::new().fg(self.fg_muted);
        (key, label)
    }
}
