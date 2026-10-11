pub mod components;
pub mod details;
pub mod help;
pub mod kanban;
pub mod keys;
pub mod modal;
pub mod sections;
pub mod text;
pub mod theme;

use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{Model, Problem, SyncStatus};

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

/// What the chrome shows beside the board: where the data lives and which sync
/// remote is set. The model does not hold either, so every renderer receives
/// them through this one value.
#[derive(Debug, Clone, Copy)]
pub struct Chrome<'a> {
    pub data_path: &'a Path,
    pub remote: Option<&'a str>,
}

/// Draw the whole UI. The only value a render produces is the help overlay's
/// clamped scroll offset, which the caller stores; nothing here mutates state.
pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, chrome: &Chrome) -> Option<usize> {
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
    modal::draw(f, model, theme, chrome)
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

    let [left_area, _gap, right_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(gap),
        Constraint::Length(right_width.min(area.width)),
    ])
    .areas(area);

    // A board that could not be read takes the hint line's place, so it is
    // unmissable while the sync status stays visible on the right.
    let left =
        board_damage_line(model, theme, left_area.width).unwrap_or_else(|| hint_line(model, theme));
    f.render_widget(Paragraph::new(left), left_area);
    if let Some(line) = right {
        f.render_widget(Paragraph::new(line), right_area);
    }
}

/// A small dot: blinking accent while a sync is in flight. A sync failure is a
/// problem like any other, shown through the same line; idle and local-only show
/// nothing.
fn status_line(model: &Model, theme: &Theme) -> Option<Line<'static>> {
    if let Some(problem) = &model.ui.sync_problem {
        return Some(problem_line(problem, theme));
    }
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

/// A write error takes priority over everything else on the right. The full
/// text is kept for the details overlay, so the footer stays one short line.
fn error_line(model: &Model, theme: &Theme) -> Option<Line<'static>> {
    model
        .ui
        .error
        .as_ref()
        .map(|problem| problem_line(problem, theme))
}

/// The footer line for a board file that could not be read, or `None` when the
/// board is intact. The backup path and the reason are in the details overlay.
fn board_damage_line(model: &Model, theme: &Theme, width: u16) -> Option<Line<'static>> {
    let problem = model.ui.board_damage.as_ref()?;
    let text = problem_text(problem);
    Some(Line::from(Span::styled(
        text::truncate(&text, width as usize),
        theme.warn(),
    )))
}

/// One marker for every problem, shared by the footer line and the overlay
/// heading, so the same failure looks the same wherever it is shown.
pub(crate) const PROBLEM_MARK: &str = "\u{26a0}";

/// A footer line for a problem: the short summary and the key that opens the
/// details overlay, so every failure reads the same way.
fn problem_line(problem: &Problem, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(problem_text(problem), theme.warn()))
}

fn problem_text(problem: &Problem) -> String {
    format!(" {PROBLEM_MARK} {} \u{b7} ! details ", problem.summary)
}

fn line_width(line: &Line) -> u16 {
    line.spans
        .iter()
        .map(|span| span.content.width())
        .sum::<usize>() as u16
}

fn hint_line(model: &Model, theme: &Theme) -> Line<'static> {
    let (key_style, label_style) = theme.key_hint();
    let hints = keys::hints(model.ui.mode);

    let mut spans = Vec::new();
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", theme.disabled()));
        }
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, label_style));
    }
    Line::from(spans)
}
