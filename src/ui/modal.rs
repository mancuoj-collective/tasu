use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Mode};

pub fn draw(f: &mut Frame, app: &App) {
    match app.mode {
        Mode::Add | Mode::Edit => input_modal(f, app),
        Mode::ConfirmDelete => confirm_modal(f, app),
        _ => {}
    }
}

fn input_modal(f: &mut Frame, app: &App) {
    let t = app.theme;
    let area = centered(f.area(), 50, 3);
    f.render_widget(Clear, area);

    let title = if app.mode == Mode::Add {
        " add "
    } else {
        " edit "
    };
    let block = Block::bordered()
        .border_style(t.border(true))
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, t.title()));

    let inner = block.inner(area);
    let scroll = app.input.visual_scroll(inner.width as usize);
    f.render_widget(
        Paragraph::new(app.input.value())
            .style(t.text())
            .scroll((0, scroll as u16))
            .block(block),
        area,
    );
    let x = inner.x + (app.input.visual_cursor().max(scroll) - scroll) as u16;
    f.set_cursor_position((x, inner.y));
}

fn confirm_modal(f: &mut Frame, app: &App) {
    let t = app.theme;
    let area = centered(f.area(), 40, 3);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(t.border(true))
        .padding(Padding::horizontal(1))
        .title(Span::styled(" confirm ", t.title()));

    let budget = block.inner(area).width as usize;
    let msg = match app.todos.selected_title() {
        Some(todo) => {
            let delimiter = "\"";
            let prefix = Span::styled("delete ", t.text());
            let suffix = Span::styled("?", t.text());
            let reserved = prefix.width() + suffix.width() + delimiter.width() * 2;
            let title = truncate(todo, budget.saturating_sub(reserved));
            Line::from(vec![
                prefix,
                Span::styled(format!("{delimiter}{title}{delimiter}"), t.error),
                suffix,
            ])
        }
        None => Line::from("nothing selected").style(t.muted()),
    };
    f.render_widget(Paragraph::new(msg).block(block), area);
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

fn truncate(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    let mut truncated = String::new();
    let mut width = 0;
    for c in text.chars() {
        let char_width = c.width().unwrap_or(0);
        if width + char_width > max_width.saturating_sub(1) {
            break;
        }
        width += char_width;
        truncated.push(c);
    }
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::truncate;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn keeps_text_that_fits() {
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hi", 10), "hi");
        assert_eq!(truncate("hello world", 5), "hell…");
        assert_eq!(truncate("你好世界", 5), "你好…");
    }

    #[test]
    fn result_never_exceeds_max_width() {
        for text in ["hello world", "你好世界", "a b c d e f"] {
            for max_width in 1..=10 {
                let out = truncate(text, max_width);
                assert!(
                    out.width() <= max_width,
                    "truncate({text:?}, {max_width}) = {out:?} ({} columns)",
                    out.width()
                );
            }
        }
    }
}
