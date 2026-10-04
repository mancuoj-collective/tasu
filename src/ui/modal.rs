use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{HistoryView, Mode, Model};

use unicode_width::UnicodeWidthStr;

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
        Mode::Help => help_modal(f, model, theme, data_path),
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
    // A fixed default height so switching tabs never resizes the box.
    let height = 18u16.min(f.area().height.saturating_sub(2));
    let area = centered(f.area(), 60, height);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1));
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
            Paragraph::new("\u{00af}\\_(._.)_/\u{00af}")
                .style(theme.muted())
                .centered(),
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
            Style::new().bg(theme.accent).fg(theme.sel_bg).bold()
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

fn help_modal(f: &mut Frame, model: &Model, theme: &Theme, data_path: &Path) {
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
        ("c", "history"),
        ("tab", "history: done / dropped (or \u{2190}\u{2192})"),
        ("q / esc", "quit"),
    ];

    let full = f.area();
    let width = full.width.saturating_sub(4).min(50);
    // Content width inside the border (2) and horizontal padding (2 per side).
    let inner = width.saturating_sub(6) as usize;

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
    let label = "data";
    let available = inner.saturating_sub(label.chars().count() + 2);
    let path = truncate_middle(&display_path(data_path), available);
    let data = format!("{label}  {path}");
    let data_pad = inner.saturating_sub(data.width()) / 2;
    lines.push(Line::from(Span::styled(
        format!("{}{data}", " ".repeat(data_pad)),
        label_style,
    )));

    let padding = Padding::new(2, 2, 1, 1);
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(padding);
    // Height fits the content but never exceeds the screen (one blank row to
    // spare top and bottom); extra lines scroll.
    let height = (lines.len() as u16 + 4).min(full.height.saturating_sub(2));
    let area = centered(full, width, height);
    f.render_widget(Clear, area);

    let inner_height = height.saturating_sub(4) as usize;
    let max_offset = lines.len().saturating_sub(inner_height);
    let offset = model.ui.help_scroll.get().min(max_offset);
    // Write the clamped offset back so the next key press starts from here.
    model.ui.help_scroll.set(offset);
    let visible: Vec<Line> = lines.into_iter().skip(offset).collect();
    f.render_widget(Paragraph::new(visible).block(block), area);
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

/// Keep the head and the tail (usually the filename), eliding the middle, so a
/// long path stays readable and never exceeds `max` columns.
fn truncate_middle(text: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    if text.width() <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }

    let budget = max - 1;
    let head_budget = budget / 2;
    let tail_budget = budget - head_budget;

    let mut head = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > head_budget {
            break;
        }
        used += width;
        head.push(ch);
    }

    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for ch in text.chars().rev() {
        let width = ch.width().unwrap_or(0);
        if used + width > tail_budget {
            break;
        }
        used += width;
        tail.push(ch);
    }
    tail.reverse();

    format!("{head}\u{2026}{}", tail.into_iter().collect::<String>())
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    // Keep one blank row above and below so a tall modal never touches the
    // screen edges.
    let height = height.min(area.height.saturating_sub(2));
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}

#[cfg(test)]
mod tests {
    use super::truncate_middle;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn short_paths_pass_through() {
        assert_eq!(truncate_middle("a/b/c", 20), "a/b/c");
    }

    #[test]
    fn long_paths_elide_the_middle() {
        let path = "/Users/mancuoj/Library/Application Support/tasu/todos.json";
        let out = truncate_middle(path, 24);
        assert!(out.contains('\u{2026}'));
        assert!(out.starts_with('/'), "keeps the head: {out}");
        assert!(out.ends_with("todos.json"), "keeps the tail: {out}");
        assert!(out.width() <= 24);
    }
}
