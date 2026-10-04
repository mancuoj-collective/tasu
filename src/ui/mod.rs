pub mod components;
pub mod kanban;
pub mod modal;
pub mod sections;
pub mod theme;

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

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme) {
    f.render_widget(Block::new().style(theme.root()), f.area());

    let body = f.area().inner(Margin::new(1, 0));
    let [header, list, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(body);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled("tasu", theme.accent()))),
        header,
    );

    if list.width >= KANBAN_MIN_WIDTH {
        kanban::draw(f, model, list, theme);
    } else {
        sections::draw(f, model, list, theme);
    }
    footer(f, model, theme, footer_area);
    modal::draw(f, model, theme);
}

fn footer(f: &mut Frame, model: &Model, theme: &Theme, area: ratatui::layout::Rect) {
    let toast = model
        .ui
        .toast
        .as_ref()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    let toast_width = toast.chars().count() as u16 + 2;
    let [left, right] = Layout::horizontal([
        Constraint::Min(1),
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
            ("q", "退出"),
            ("j/k", "移动"),
            ("space", "完成"),
            ("a", "记录"),
            ("t", "今天"),
            ("[/]", "升降"),
            ("x", "归档"),
            ("l", "折叠"),
            ("c", "已完成"),
            ("?", "帮助"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "保存"), ("esc", "取消")],
        Mode::Completed => &[("输入", "搜索"), ("enter", "撤销"), ("esc", "返回")],
        Mode::Help => &[("任意键", "关闭")],
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
