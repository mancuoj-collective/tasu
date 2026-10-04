use ratatui::style::{Color, Style};

/// Cold palette. Color is a signal, never decoration: only the selection, the
/// `Today` bucket and stale-task warnings are allowed to stand out.
#[derive(Clone, Copy)]
pub struct Theme {
    pub fg: Color,
    pub muted: Color,
    pub disabled: Color,
    pub accent: Color,
    pub today: Color,
    pub warn: Color,
    pub success: Color,
    pub sel_bg: Color,
}

impl Theme {
    pub const DARK: Self = Self {
        fg: Color::Rgb(0xc0, 0xca, 0xf5),
        muted: Color::Rgb(0x7a, 0x83, 0xad),
        disabled: Color::Rgb(0x3b, 0x42, 0x61),
        accent: Color::Rgb(0x7a, 0xa2, 0xf7),
        today: Color::Rgb(0x7d, 0xcf, 0xff),
        warn: Color::Rgb(0xe0, 0xaf, 0x68),
        success: Color::Rgb(0x9e, 0xce, 0x6a),
        sel_bg: Color::Rgb(0x29, 0x2e, 0x42),
    };

    pub const LIGHT: Self = Self {
        fg: Color::Rgb(0x37, 0x60, 0xbf),
        muted: Color::Rgb(0x6b, 0x73, 0x94),
        disabled: Color::Rgb(0xa1, 0xa6, 0xc5),
        accent: Color::Rgb(0x2e, 0x7d, 0xe9),
        today: Color::Rgb(0x00, 0x71, 0x97),
        warn: Color::Rgb(0xb1, 0x5c, 0x00),
        success: Color::Rgb(0x58, 0x75, 0x39),
        sel_bg: Color::Rgb(0xc4, 0xc8, 0xda),
    };

    pub fn detect() -> Self {
        match terminal_colorsaurus::theme_mode(terminal_colorsaurus::QueryOptions::default()) {
            Ok(terminal_colorsaurus::ThemeMode::Light) => Self::LIGHT,
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

    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent).bold()
    }

    pub fn today(&self) -> Style {
        Style::new().fg(self.today).bold()
    }

    pub fn warn(&self) -> Style {
        Style::new().fg(self.warn)
    }

    pub fn success(&self) -> Style {
        Style::new().fg(self.success)
    }

    pub fn highlight(&self) -> Style {
        Style::new().fg(self.accent).bg(self.sel_bg).bold()
    }

    pub fn key_hint(&self) -> (Style, Style) {
        let key = Style::new().fg(self.accent).bold();
        let label = Style::new().fg(self.muted);
        (key, label)
    }
}
