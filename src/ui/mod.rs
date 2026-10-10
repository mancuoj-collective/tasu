pub mod components;
pub mod kanban;
pub mod modal;
pub mod sections;
pub mod theme;

use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{Mode, Model, SyncStatus};

use unicode_width::UnicodeWidthStr;

use theme::Theme;

/// At or above this width, the three buckets become side-by-side columns.
const KANBAN_MIN_WIDTH: u16 = 100;
/// Below either bound the normal UI is unusable, so we ask for a resize.
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 8;

/// Keep the cursor inside a `height`-row window starting at the returned offset.
pub(crate) fn scroll_offset(cursor: usize, total: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    cursor
        .saturating_sub(height - 1)
        .min(total.saturating_sub(height))
}

/// Draw the whole UI. The only value a render produces is the help overlay's
/// clamped scroll offset, which the caller stores; nothing here mutates state.
pub fn draw(
    f: &mut Frame,
    model: &Model,
    theme: &Theme,
    data_path: &Path,
    sync: Option<&str>,
) -> Option<usize> {
    f.render_widget(Block::new().style(theme.root()), f.area());

    if f.area().width < MIN_WIDTH || f.area().height < MIN_HEIGHT {
        too_small(f, theme);
        return None;
    }

    let body = f.area().inner(Margin::new(1, 0));
    let [list, footer_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);

    if list.width >= KANBAN_MIN_WIDTH {
        kanban::draw(f, model, list, theme);
    } else {
        sections::draw(f, model, list, theme);
    }
    footer(f, model, theme, footer_area);
    modal::draw(f, model, theme, data_path, sync)
}

fn too_small(f: &mut Frame, theme: &Theme) {
    let area = f.area();
    let [middle] = Layout::vertical([Constraint::Length(3)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    let lines = [
        Line::from(Span::styled("terminal too small", theme.accent())),
        Line::from(Span::styled(
            format!("enlarge to at least {MIN_WIDTH}x{MIN_HEIGHT}"),
            theme.muted(),
        )),
        Line::from(Span::styled("q to quit", theme.muted())),
    ];
    f.render_widget(Paragraph::new(lines.to_vec()).centered(), middle);
}

fn footer(f: &mut Frame, model: &Model, theme: &Theme, area: ratatui::layout::Rect) {
    // Right side: a write error wins, then sync progress, then the toast.
    let right = error_line(model, theme)
        .or_else(|| status_line(model, theme))
        .or_else(|| toast_line(model, theme));
    let right_width = right.as_ref().map(line_width).unwrap_or(0);
    let gap = if right_width > 0 { 2 } else { 0 };

    let [left, _gap, right_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(gap),
        Constraint::Length(right_width.min(area.width)),
    ])
    .areas(area);

    f.render_widget(Paragraph::new(hint_line(model, theme)), left);
    if let Some(line) = right {
        f.render_widget(Paragraph::new(line), right_area);
    }
}

/// A small dot: blinking accent while a sync is in flight, steady warning if
/// the last one failed. Idle and local-only show nothing.
fn status_line(model: &Model, theme: &Theme) -> Option<Line<'static>> {
    match model.ui.sync {
        SyncStatus::Syncing => {
            let on = (model.ui.tick / 3).is_multiple_of(2);
            let span = if on {
                Span::styled("\u{25cf}", theme.accent())
            } else {
                Span::raw(" ")
            };
            Some(Line::from(span))
        }
        SyncStatus::Failed => {
            let reason = model
                .ui
                .sync_error
                .as_deref()
                .map(|err| crate::sync::Failure::classify(err).footer())
                .unwrap_or("sync failed");
            Some(Line::from(Span::styled(
                format!("\u{2717} {reason} \u{b7} run tasu sync"),
                theme.warn(),
            )))
        }
        SyncStatus::Local | SyncStatus::Idle => None,
    }
}

fn toast_line(model: &Model, theme: &Theme) -> Option<Line<'static>> {
    model
        .ui
        .toast
        .as_ref()
        .map(|toast| Line::from(Span::styled(format!(" {} ", toast.text), theme.success())))
}

/// A write error takes priority over everything else on the right.
fn error_line(model: &Model, theme: &Theme) -> Option<Line<'static>> {
    model.ui.error.as_ref().map(|message| {
        let short = components::truncate(message, 60);
        Line::from(Span::styled(format!(" \u{26a0} {short} "), theme.warn()))
    })
}

fn line_width(line: &Line) -> u16 {
    line.spans
        .iter()
        .map(|span| span.content.width())
        .sum::<usize>() as u16
}

fn hint_line(model: &Model, theme: &Theme) -> Line<'static> {
    let (key_style, label_style) = theme.key_hint();
    let hints: &[(&str, &str)] = match model.ui.mode {
        Mode::Normal => &[
            ("?", "help"),
            ("q", "quit"),
            ("a", "add"),
            ("c", "history"),
            ("[]", "move"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "save"), ("esc", "cancel")],
        Mode::Completed => &[
            ("\u{2190}\u{2192}", "switch"),
            ("enter", "restore"),
            ("\u{2191}\u{2193}", "move"),
            ("esc", "back"),
        ],
        Mode::Help => &[("\u{2191}\u{2193}", "scroll"), ("esc", "close")],
    };

    let mut spans = Vec::new();
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", theme.disabled()));
        }
        spans.push(Span::styled((*key).to_string(), key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*label).to_string(), label_style));
    }
    Line::from(spans)
}
