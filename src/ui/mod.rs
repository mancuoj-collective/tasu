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

use crate::app::{Mode, Model};

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

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, data_path: &Path) {
    f.render_widget(Block::new().style(theme.root()), f.area());

    if f.area().width < MIN_WIDTH || f.area().height < MIN_HEIGHT {
        too_small(f, theme);
        return;
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
    modal::draw(f, model, theme, data_path);
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
    let toast = model
        .ui
        .toast
        .as_ref()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    let toast_width = if toast.is_empty() {
        0
    } else {
        toast.chars().count() as u16 + 2
    };
    let gap = if toast_width > 0 { 2 } else { 0 };

    let [left, _gap, right] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(gap),
        Constraint::Length(toast_width.min(area.width)),
    ])
    .areas(area);

    f.render_widget(Paragraph::new(hint_line(model, theme)), left);
    if !toast.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {toast} "), theme.success())),
            right,
        );
    }
}

fn hint_line(model: &Model, theme: &Theme) -> Line<'static> {
    let (key_style, label_style) = theme.key_hint();
    let hints: &[(&str, &str)] = match model.ui.mode {
        Mode::Normal => &[
            ("?", "help"),
            ("q", "quit"),
            ("a", "add"),
            ("c", "history"),
            ("[/]", "bucket"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "save"), ("esc", "cancel")],
        Mode::Completed => &[
            ("tab", "done/dropped"),
            ("enter", "restore"),
            ("\u{2191}\u{2193}", "move"),
            ("esc", "back"),
        ],
        Mode::Help => &[("any key", "close")],
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
