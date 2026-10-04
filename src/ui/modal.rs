use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{HistoryView, Mode, Model};

use super::{components, scroll_offset, theme::Theme};

/// Compact block-letter `tasu`, shown at the top of help. Three rows keeps it
/// from dominating the modal while the `s` stays legible.
const HELP_ART: [&str; 3] = [
    "\u{2580}\u{2588}\u{2580} \u{2584}\u{2580}\u{2588} \u{2584}\u{2580}\u{2580} \u{2588} \u{2588}",
    " \u{2588}  \u{2588}\u{2580}\u{2588} \u{2580}\u{2580}\u{2584} \u{2588} \u{2588}",
    " \u{2580}  \u{2580} \u{2580} \u{2584}\u{2584}\u{2580} \u{2580}\u{2584}\u{2580}",
];

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, data_path: &Path) {
    match model.ui.mode {
        Mode::Add | Mode::Edit => input_modal(f, model, theme),
        Mode::Completed => completed_modal(f, model, theme),
        Mode::Help => help_modal(f, theme, data_path),
        Mode::Normal => {}
    }
}

fn input_modal(f: &mut Frame, model: &Model, theme: &Theme) {
    let area = centered(f.area(), 50, 3);
    f.render_widget(Clear, area);

    let title = if model.ui.mode == Mode::Add {
        " add "
    } else {
        " edit "
    };
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, theme.accent()));

    let inner = block.inner(area);
    let scroll = model.ui.input.visual_scroll(inner.width as usize);
    f.render_widget(
        Paragraph::new(model.ui.input.value())
            .style(theme.text())
            .scroll((0, scroll as u16))
            .block(block),
        area,
    );
    let x = inner.x + (model.ui.input.visual_cursor().max(scroll) - scroll) as u16;
    f.set_cursor_position((x, inner.y));
}

fn completed_modal(f: &mut Frame, model: &Model, theme: &Theme) {
    let items = model.history_items();
    // Sized by the unfiltered total so the box does not jump while typing.
    let total = model.history_source().len();
    let height = (total.min(12) as u16 + 6).clamp(9, 21);
    let area = centered(f.area(), 60, height);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" history ", theme.accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [tabs_area, search_area, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(1),
    ])
    .areas(inner);

    f.render_widget(Paragraph::new(history_tabs(model, theme)), tabs_area);
    search_box(f, model, theme, search_area);

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("\u{2205}").style(theme.muted()).centered(),
            list_area,
        );
        return;
    }

    let height = list_area.height as usize;
    let offset = scroll_offset(model.ui.done_cursor, items.len(), height);
    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(position, &index)| {
            let Some(task) = model.board.task(index) else {
                return Line::default();
            };
            let mut line = components::task_line(task, theme, list_area.width);
            if position == model.ui.done_cursor {
                line = components::pad_line(line, list_area.width, theme.highlight());
                line = line.style(theme.highlight());
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), list_area);
}

fn history_tabs(model: &Model, theme: &Theme) -> Line<'static> {
    let tab = |label: &'static str, active: bool| {
        let style = if active {
            Style::new().fg(theme.accent).underlined()
        } else {
            theme.muted()
        };
        Span::styled(format!(" {label} "), style)
    };
    Line::from(vec![
        tab("DONE", model.ui.history_view == HistoryView::Done),
        Span::raw(" "),
        tab("DROPPED", model.ui.history_view == HistoryView::Dropped),
    ])
}

/// A bordered search field with a real cursor, fixed above the scrolling list.
fn search_box(f: &mut Frame, model: &Model, theme: &Theme, area: Rect) {
    let block = Block::bordered()
        .border_style(theme.disabled())
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);

    let input = &model.ui.done_filter;
    let text = if input.value().is_empty() {
        Paragraph::new("search").style(theme.muted())
    } else {
        Paragraph::new(input.value()).style(theme.text())
    };
    let scroll = input.visual_scroll(inner.width as usize);
    f.render_widget(text.scroll((0, scroll as u16)).block(block), area);

    let x = inner.x + (input.visual_cursor().max(scroll) - scroll) as u16;
    f.set_cursor_position((x, inner.y));
}

fn help_modal(f: &mut Frame, theme: &Theme, data_path: &Path) {
    let (key_style, label_style) = theme.key_hint();
    let entries = [
        ("j / k", "move"),
        ("h / l", "previous / next section"),
        ("g / G", "top / bottom"),
        ("space", "done"),
        ("a", "add (to today)"),
        ("e", "edit title"),
        ("t", "move to today"),
        ("[ / ]", "send to previous / next bucket"),
        ("x", "drop (archive)"),
        ("z", "expand / fold later"),
        ("c", "history"),
        ("tab", "history: done / dropped"),
        ("q / esc", "quit"),
    ];

    let full = f.area();
    let width = full.width.saturating_sub(4).min(48);
    // Content width inside the border (2) and horizontal padding (2).
    let inner = width.saturating_sub(4) as usize;

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
    lines.push(Line::default());

    let label_width = inner.saturating_sub(10);
    for (key, label) in entries {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<10}"), key_style),
            Span::styled(format!("{label:>label_width$}"), label_style),
        ]));
    }

    lines.push(Line::default());
    let data = format!("data  {}", data_path.display());
    let data = truncate_start(&data, inner);
    let data_pad = inner.saturating_sub(data.chars().count()) / 2;
    lines.push(Line::from(Span::styled(
        format!("{}{data}", " ".repeat(data_pad)),
        label_style,
    )));

    let height = (lines.len() as u16 + 2).min(full.height);
    let area = centered(full, width, height);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Keep the tail of a long path (the meaningful part) and mark the cut.
fn truncate_start(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }
    let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
    format!("\u{2026}{tail}")
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
