//! The details overlay: the full, untruncated text of whatever went wrong, so a
//! long IO error or the path of a backup is readable even in a narrow terminal.
//! The footer only carries the short summary that opens this.

use ratatui::{
    Frame,
    layout::Alignment,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::Model;

use super::help::push_block;
use super::modal::centered;
use super::theme::Theme;

const DETAILS_MAX_WIDTH: u16 = 78;
const DETAILS_PAD_X: u16 = 2;
const DETAILS_PAD_Y: u16 = 1;
/// Border (2) plus vertical padding, matching `help`.
const DETAILS_CHROME: u16 = 2 + DETAILS_PAD_Y * 2;

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
        lines.push(Line::from(Span::styled(
            problem.summary.clone(),
            theme.warn(),
        )));
        // The detail wraps onto continuation lines, so nothing is elided.
        push_block(
            &mut lines,
            "  ",
            &problem.detail,
            theme.text(),
            inner,
            Alignment::Left,
        );
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("nothing to report", theme.muted())));
    }

    let padding = Padding::new(DETAILS_PAD_X, DETAILS_PAD_X, DETAILS_PAD_Y, DETAILS_PAD_Y);
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(padding)
        .title(Span::styled(" details ", theme.accent()));
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
