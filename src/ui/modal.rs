use std::path::Path;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{HistoryView, Mode, Model};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{components, scroll_offset, theme::Theme};

/// Compact block-letter `tasu`, shown at the top of help. Three rows keeps it
/// from dominating the modal while the `s` stays legible.
const HELP_ART: [&str; 3] = [
    "\u{2580}\u{2588}\u{2580} \u{2584}\u{2580}\u{2588} \u{2584}\u{2580}\u{2580} \u{2588} \u{2588}",
    " \u{2588}  \u{2588}\u{2580}\u{2588} \u{2580}\u{2580}\u{2584} \u{2588} \u{2588}",
    " \u{2580}  \u{2580} \u{2580} \u{2584}\u{2584}\u{2580} \u{2580}\u{2584}\u{2580}",
];

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, data_path: &Path, sync: Option<&str>) {
    match model.ui.mode {
        Mode::Add | Mode::Edit => input_modal(f, model, theme),
        Mode::Completed => completed_modal(f, model, theme),
        Mode::Help => help_modal(f, model, theme, data_path, sync),
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

fn help_modal(f: &mut Frame, model: &Model, theme: &Theme, data_path: &Path, sync: Option<&str>) {
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
        ("ctrl+c", "quit anywhere"),
    ];

    let full = f.area();
    let width = full.width.saturating_sub(4).min(56);
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

    // Keep the sync error at the top so it is visible even on short terminals.
    // Wrap it in full: a half-shown git error is useless for diagnosis.
    if let Some(err) = &model.ui.sync_error {
        push_block(&mut lines, "!  ", err, theme.warn(), inner);
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
    // Wrap the long, space-less values (paths, URLs) onto continuation lines
    // with a hanging indent instead of eliding them.
    push_block(
        &mut lines,
        "data  ",
        &display_path(data_path),
        label_style,
        inner,
    );
    let sync = match sync {
        Some(url) => ("sync  ", url.to_string()),
        None => ("sync  ", "off \u{b7} tasu remote <url>".to_string()),
    };
    push_block(&mut lines, sync.0, &sync.1, label_style, inner);

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

/// Append a labelled, wrapped block to `lines`. The first line begins with
/// `prefix`; continuation lines are indented under it. Long tokens without
/// spaces (paths, URLs) are hard-broken, so nothing is ever cut off.
fn push_block(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    body: &str,
    style: Style,
    inner: usize,
) {
    let indent = prefix.width();
    let body_width = inner.saturating_sub(indent).max(1);
    for (i, chunk) in wrap_text(body, body_width).into_iter().enumerate() {
        let line = if i == 0 {
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(chunk, style),
            ])
        } else {
            Line::from(Span::styled(
                format!("{}{chunk}", " ".repeat(indent)),
                style,
            ))
        };
        lines.push(line);
    }
}

/// Greedy word wrap to at most `width` columns, measured in display cells.
/// Words longer than a line are hard-broken. Always returns at least one line.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out: Vec<String> = Vec::new();

    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut line_width = 0usize;
        for word in paragraph.split_whitespace() {
            let mut rest = word;
            while !rest.is_empty() {
                let space = usize::from(line_width > 0);
                if line_width + space >= width {
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                    continue;
                }
                let avail = width - line_width - space;
                // Prefer moving a whole word down over splitting it, unless it
                // is longer than a full line and must be broken anyway.
                if space == 1 && rest.width() > avail && rest.width() <= width {
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                    continue;
                }
                let (chunk, tail) = split_at_width(rest, avail);
                if space == 1 {
                    line.push(' ');
                    line_width += 1;
                }
                line_width += chunk.width();
                line.push_str(&chunk);
                rest = tail;
                if !rest.is_empty() {
                    // The chunk filled the line; flush before continuing.
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                }
            }
        }
        out.push(line);
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Split `text` at the widest prefix that fits in `width` cells. Takes one
/// character even if it overflows, so the caller always makes progress.
fn split_at_width(text: &str, width: usize) -> (String, &str) {
    let mut used = 0usize;
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > width {
            break;
        }
        used += cells;
        end = index + ch.len_utf8();
    }
    if end == 0 && !text.is_empty() {
        end = text.chars().next().map(char::len_utf8).unwrap_or(0);
    }
    (text[..end].to_string(), &text[end..])
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
    use super::wrap_text;
    use unicode_width::UnicodeWidthStr;

    fn widths(lines: &[String]) -> Vec<usize> {
        lines.iter().map(|line| line.width()).collect()
    }

    #[test]
    fn short_text_is_a_single_line() {
        assert_eq!(wrap_text("a/b/c", 20), vec!["a/b/c".to_string()]);
    }

    #[test]
    fn words_wrap_on_spaces_without_splitting_them() {
        let text = "fatal: repository not found";
        let lines = wrap_text(text, 12);
        assert!(lines.len() > 1, "should wrap: {lines:?}");
        assert!(widths(&lines).iter().all(|&w| w <= 12), "{lines:?}");
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn long_tokens_are_hard_broken_without_loss() {
        let token = "https://github.com/mancuoj-collective/tasu-data.git";
        let lines = wrap_text(token, 24);
        assert!(lines.len() > 1, "should wrap: {lines:?}");
        assert!(widths(&lines).iter().all(|&w| w <= 24), "{lines:?}");
        assert_eq!(lines.concat(), token);
    }

    #[test]
    fn wrapping_counts_display_cells() {
        // CJK characters occupy two cells each.
        let lines = wrap_text("数据数据数据", 5);
        assert!(widths(&lines).iter().all(|&w| w <= 5), "{lines:?}");
        assert_eq!(lines.concat(), "数据数据数据");
    }

    #[test]
    fn empty_text_still_yields_one_line() {
        assert_eq!(wrap_text("", 10), vec![String::new()]);
    }
}
