//! The help overlay: the key list, the logo, and where the data lives and which
//! remote is set. Its key list is shared with the footer through `keys`.

use std::path::Path;

use ratatui::{
    Frame,
    layout::Alignment,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::Model;

use super::modal::centered;
use super::text::wrap_text;
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
    // Height fits the content but never exceeds the screen (one blank row to
    // spare top and bottom); extra lines scroll.
    let height = (lines.len() as u16 + HELP_CHROME).min(full.height.saturating_sub(2));
    let area = centered(full, width, height);
    f.render_widget(Clear, area);

    let inner_height = height.saturating_sub(HELP_CHROME) as usize;
    let max_offset = lines.len().saturating_sub(inner_height);
    let offset = model.ui.overlay_scroll.min(max_offset);
    let visible: Vec<Line> = lines.into_iter().skip(offset).collect();
    f.render_widget(Paragraph::new(visible).block(block), area);
    offset
}

/// Abbreviate the home directory to `~`.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(home) = dirs::home_dir() {
        let home = home.display().to_string();
        if let Some(rest) = text.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    text
}

/// Append a labelled, wrapped block to `lines`. The first line begins with
/// `prefix`; continuation lines are indented under it. Long tokens without
/// spaces (paths, URLs) are hard-broken, so nothing is ever cut off.
pub(super) fn push_block(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    body: &str,
    style: Style,
    inner: usize,
    align: Alignment,
) {
    let indent = prefix.width();
    let body_width = inner.saturating_sub(indent).max(1);
    for (i, chunk) in wrap_text(body, body_width).into_iter().enumerate() {
        let chunk = chunk.trim_end();
        let lead = if i == 0 {
            prefix.to_string()
        } else {
            " ".repeat(indent)
        };
        let pad = match align {
            Alignment::Right => body_width.saturating_sub(chunk.width()),
            Alignment::Center => body_width.saturating_sub(chunk.width()) / 2,
            Alignment::Left => 0,
        };
        lines.push(Line::from(Span::styled(
            format!("{lead}{}{chunk}", " ".repeat(pad)),
            style,
        )));
    }
}
