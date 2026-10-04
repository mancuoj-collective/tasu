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
    let show_help = model.ui.mode == Mode::Normal;
    let help_width = if show_help { 8 } else { 0 };

    let [left, toast_area, help_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(toast_width.min(area.width)),
        Constraint::Length(help_width.min(area.width)),
    ])
    .areas(area);

    // Left hints may be truncated on narrow terminals; `? help` stays pinned.
    f.render_widget(Paragraph::new(hint_line(model, theme)), left);

    if !toast.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {toast} "), theme.success())),
            toast_area,
        );
    }

    if show_help {
        let (key_style, label_style) = theme.key_hint();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("?", key_style),
                Span::raw(" "),
                Span::styled("help", label_style),
            ])),
            help_area,
        );
    }
}

fn hint_line(model: &Model, theme: &Theme) -> Line<'static> {
    let (key_style, label_style) = theme.key_hint();
    let hints: &[(&str, &str)] = match model.ui.mode {
        Mode::Normal => &[
            ("q", "quit"),
            ("j/k", "move"),
            ("space", "done"),
            ("a", "add"),
            ("[/]", "bucket"),
            ("x", "archive"),
            ("c", "history"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "save"), ("esc", "cancel")],
        Mode::Completed => &[("type", "search"), ("enter", "restore"), ("esc", "back")],
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
