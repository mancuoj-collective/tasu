//! The help overlay: the key list, the logo, and where the data lives and which
//! remote is set. Its key list is shared with the footer through `keys`.

use std::path::Path;

use ratatui::{
    Frame,
    layout::Alignment,
    text::{Line, Span},
    widgets::{Block, Padding},
};

use crate::app::Model;

use super::modal::render_overlay;
use super::text::push_block;
use super::{Chrome, keys, theme::Theme};

/// Help overlay geometry, in cells. Generous side padding keeps the content off
/// the border; the logo gets no margin above and a blank line below.
const HELP_MAX_WIDTH: u16 = 62;
const HELP_PAD_X: u16 = 3;
const HELP_PAD_TOP: u16 = 1;
const HELP_PAD_BOTTOM: u16 = 1;
/// Width of the key column, wide enough for `g / G / home / end`.
const HELP_KEY_COL: usize = 18;
/// Border (2) plus vertical padding.
const HELP_CHROME: u16 = 2 + HELP_PAD_TOP + HELP_PAD_BOTTOM;

/// Compact block-letter `tasu`, shown at the top of help. Three rows keeps it
/// from dominating the modal while the `s` stays legible.
const HELP_ART: [&str; 3] = [
    "\u{2580}\u{2588}\u{2580} \u{2584}\u{2580}\u{2588} \u{2584}\u{2580}\u{2580} \u{2588} \u{2588}",
    " \u{2588}  \u{2588}\u{2580}\u{2588} \u{2580}\u{2580}\u{2584} \u{2588} \u{2588}",
    " \u{2580}  \u{2580} \u{2580} \u{2584}\u{2584}\u{2580} \u{2580}\u{2584}\u{2580}",
];

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, chrome: &Chrome) -> usize {
    let data_path = chrome.data_path;
    let sync = chrome.remote;
    let (key_style, label_style) = theme.key_hint();

    let full = f.area();
    let width = full.width.saturating_sub(4).min(HELP_MAX_WIDTH);
    // Content width inside the border (2) and horizontal padding.
    let inner = width.saturating_sub(2 + HELP_PAD_X * 2) as usize;

    let mut lines: Vec<Line> = Vec::new();
    // Center the logo by its widest row so the letters stay aligned.
    let art_width = HELP_ART
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0);
    let art_pad = inner.saturating_sub(art_width) / 2;
    for row in HELP_ART {
        lines.push(Line::from(Span::styled(
            format!("{}{row}", " ".repeat(art_pad)),
            theme.accent(),
        )));
    }
    // Breathing room under the logo, none above it.
    lines.push(Line::default());

    let label_width = inner.saturating_sub(HELP_KEY_COL);
    for &(key, label) in keys::HELP {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<width$}", width = HELP_KEY_COL), key_style),
            Span::styled(format!("{label:>label_width$}"), label_style),
        ]));
    }

    lines.push(Line::default());
    // Wrap the long, space-less values (paths, URLs) onto continuation lines
    // with a hanging indent instead of eliding them.
    push_block(
        &mut lines,
        "data  ",
        &display_path(data_path),
        label_style,
        inner,
        Alignment::Right,
    );
    let sync = match sync {
        Some(url) => ("sync  ", url.to_string()),
        None => ("sync  ", "off \u{b7} tasu remote <url>".to_string()),
    };
    push_block(
        &mut lines,
        sync.0,
        &sync.1,
        label_style,
        inner,
        Alignment::Right,
    );

    let padding = Padding::new(HELP_PAD_X, HELP_PAD_X, HELP_PAD_TOP, HELP_PAD_BOTTOM);
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(padding);
    render_overlay(
        f,
        full,
        lines,
        width,
        HELP_CHROME,
        block,
        model.ui.overlay_scroll,
    )
}

/// Abbreviate the home directory to `~`. Only at a path boundary, so a sibling
/// whose name merely starts with the home name (`/Users/alex` vs `/Users/al`)
/// is not mangled into `~ex`. Component-based, so it works with either
/// separator.
fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::display_path;
    use std::path::MAIN_SEPARATOR;

    #[test]
    fn only_a_path_boundary_is_abbreviated() {
        let home = dirs::home_dir().expect("a home directory");
        assert_eq!(display_path(&home), "~");
        assert_eq!(
            display_path(&home.join("data").join("tasu.json")),
            format!("~{MAIN_SEPARATOR}data{MAIN_SEPARATOR}tasu.json")
        );

        // A sibling whose name starts with the home name must be left alone.
        let sibling = home.with_file_name(format!(
            "{}-other",
            home.file_name().unwrap().to_string_lossy()
        ));
        assert_eq!(display_path(&sibling), sibling.display().to_string());
    }
}
