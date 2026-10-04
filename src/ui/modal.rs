use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{Mode, Model};

use super::{components, scroll_offset, theme::Theme};

/// Block-letter `tasu` (ANSI Shadow), shown at the top of help. The `s` glyph
/// is unambiguous here, unlike the compact half-block version it replaces.
const HELP_ART: [&str; 6] = [
    " ████████╗ █████╗ ███████╗██╗   ██╗",
    " ╚══██╔══╝██╔══██╗██╔════╝██║   ██║",
    "    ██║   ███████║███████╗██║   ██║",
    "    ██║   ██╔══██║╚════██║██║   ██║",
    "    ██║   ██║  ██║███████║╚██████╔╝",
    "    ╚═╝   ╚═╝  ╚═╝╚══════╝ ╚═════╝ ",
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
    let done = model.done_filtered();
    // Sized by the unfiltered total so the box does not jump while typing.
    let total = model.board.done().len();
    let height = (total.min(12) as u16 + 5).clamp(8, 20);
    let area = centered(f.area(), 60, height);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" completed ", theme.accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [search_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(inner);
    search_box(f, model, theme, search_area);

    if done.is_empty() {
        f.render_widget(Paragraph::new("no matches").style(theme.muted()), list_area);
        return;
    }

    let height = list_area.height as usize;
    let offset = scroll_offset(model.ui.done_cursor, done.len(), height);
    let lines: Vec<Line> = done
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(position, &index)| {
            let Some(task) = model.board.task(index) else {
                return Line::default();
            };
            let mut line = components::task_line(task, theme);
            if position == model.ui.done_cursor {
                line = line.style(theme.highlight());
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), list_area);
}

/// A bordered search field with a real cursor, fixed above the scrolling list.
fn search_box(f: &mut Frame, model: &Model, theme: &Theme, area: Rect) {
    let block = Block::bordered()
        .border_style(theme.disabled())
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);

    let input = &model.ui.done_filter;
    let text = if input.value().is_empty() {
        Paragraph::new("search").style(theme.disabled())
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
        ("g / G", "top / bottom"),
        ("space", "done"),
        ("a", "add (to today)"),
        ("e", "edit title"),
        ("t", "move to today"),
        ("[ / ]", "bucket closer / farther"),
        ("x", "archive"),
        ("l", "expand / fold later"),
        ("c", "completed"),
        ("q / esc", "quit"),
    ];

    let full = f.area();
    let width = full.width.saturating_sub(4).min(48);
    // Content width inside the border (2) and horizontal padding (2).
    let inner = width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for row in HELP_ART {
        let pad = inner.saturating_sub(row.chars().count()) / 2;
        lines.push(Line::from(Span::styled(
            format!("{}{row}", " ".repeat(pad)),
            theme.accent(),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("data  ", key_style),
        Span::styled(
            truncate_start(&data_path.display().to_string(), inner.saturating_sub(6)),
            label_style,
        ),
    ]));
    lines.push(Line::default());

    let label_width = inner.saturating_sub(10);
    for (key, label) in entries {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<10}"), key_style),
            Span::styled(format!("{label:>label_width$}"), label_style),
        ]));
    }

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
