//! The details overlay: every problem in full — the summary as a heading, the
//! facts indented beneath it, and, when there is one, the next step. Nothing
//! repeats the heading and nothing is elided.

use ratatui::{
    Frame,
    layout::Alignment,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::Model;

use super::help::push_block;
use super::modal::centered;
use super::text::wrap_text;
use super::{PROBLEM_MARK, theme::Theme};

const DETAILS_MAX_WIDTH: u16 = 78;
const DETAILS_PAD_X: u16 = 2;
const DETAILS_PAD_Y: u16 = 1;
/// Border (2) plus vertical padding, matching `help`.
const DETAILS_CHROME: u16 = 2 + DETAILS_PAD_Y * 2;
/// Facts and the next step align under the summary text, past the marker.
const DETAILS_INDENT: &str = "  ";

/// Draw the overlay and return the clamped scroll offset, so the caller can
/// store it (the same contract as the help overlay).
pub fn draw(f: &mut Frame, model: &Model, theme: &Theme) -> usize {
    let full = f.area();
    let width = full.width.saturating_sub(4).min(DETAILS_MAX_WIDTH);
    // Content width inside the border (2) and horizontal padding.
    let inner = width.saturating_sub(2 + DETAILS_PAD_X * 2) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (index, problem) in model.problems().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        // A heading the colour-blind and no-colour terminals can still read: a
        // glyph and bold weight, not only the warning hue.
        lines.push(Line::from(Span::styled(
            format!("{PROBLEM_MARK} {}", problem.summary),
            theme.warn().bold(),
        )));
        // The facts wrap onto their own indented lines, so nothing is elided.
        push_block(
            &mut lines,
            DETAILS_INDENT,
            &problem.detail,
            theme.text(),
            inner,
            Alignment::Left,
        );
        if let Some(hint) = &problem.hint {
            push_hint(&mut lines, hint, theme, inner);
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("nothing to report", theme.muted())));
    }

    let padding = Padding::new(DETAILS_PAD_X, DETAILS_PAD_X, DETAILS_PAD_Y, DETAILS_PAD_Y);
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(padding)
        .title(Span::styled(" problems ", theme.accent()));
    // Height fits the content but never exceeds the screen; extra lines scroll.
    let height = (lines.len() as u16 + DETAILS_CHROME).min(full.height.saturating_sub(2));
    let area = centered(full, width, height);
    f.render_widget(Clear, area);

    let inner_height = height.saturating_sub(DETAILS_CHROME) as usize;
    let max_offset = lines.len().saturating_sub(inner_height);
    let offset = model.ui.overlay_scroll.min(max_offset);
    let visible: Vec<Line> = lines.into_iter().skip(offset).collect();
    f.render_widget(Paragraph::new(visible).block(block), area);
    offset
}

/// The next step, marked with an accent arrow and muted text so it reads as
/// advice rather than another fact. Wraps under the arrow.
fn push_hint(lines: &mut Vec<Line<'static>>, hint: &str, theme: &Theme, inner: usize) {
    const ARROW: &str = "\u{2192} ";
    let indent = DETAILS_INDENT.width();
    let body = inner.saturating_sub(indent + ARROW.width()).max(1);
    for (index, chunk) in wrap_text(hint, body).into_iter().enumerate() {
        let chunk = chunk.trim_end();
        let mut spans = vec![Span::raw(DETAILS_INDENT.to_string())];
        if index == 0 {
            spans.push(Span::styled(ARROW, theme.accent()));
        } else {
            spans.push(Span::raw(" ".repeat(ARROW.width())));
        }
        spans.push(Span::styled(chunk.to_string(), theme.muted()));
        lines.push(Line::from(spans));
    }
}
