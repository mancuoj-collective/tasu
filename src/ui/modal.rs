use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{Mode, Model};

use super::{components, scroll_offset, theme::Theme};

pub fn draw(f: &mut Frame, model: &Model, theme: &Theme) {
    match model.ui.mode {
        Mode::Add | Mode::Edit => input_modal(f, model, theme),
        Mode::Completed => completed_modal(f, model, theme),
        Mode::Help => help_modal(f, theme),
        Mode::Normal => {}
    }
}

fn input_modal(f: &mut Frame, model: &Model, theme: &Theme) {
    let area = centered(f.area(), 50, 3);
    f.render_widget(Clear, area);

    let title = if model.ui.mode == Mode::Add {
        " 记录 "
    } else {
        " 编辑 "
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
    let done = model.board.done();
    let height = (done.len() as u16 + 3).clamp(3, 20);
    let area = centered(f.area(), 60, height);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" 已完成 ", theme.accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if done.is_empty() {
        f.render_widget(Paragraph::new("还没有完成的事").style(theme.muted()), inner);
        return;
    }

    let height = inner.height as usize;
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
            let mut line = components::task_line(task, theme, model.now);
            if position == model.ui.done_cursor {
                line = line.style(theme.highlight());
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn help_modal(f: &mut Frame, theme: &Theme) {
    let area = centered(f.area(), 44, 14);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" 帮助 ", theme.accent()));

    let (key_style, label_style) = theme.key_hint();
    let entries = [
        ("j / k", "移动光标"),
        ("g / G", "跳到顶 / 底"),
        ("space", "完成"),
        ("a", "记录（进今天）"),
        ("e", "编辑标题"),
        ("t", "提到今天"),
        ("[ / ]", "升 / 降一级"),
        ("x", "归档"),
        ("c", "已完成"),
        ("q / esc", "退出"),
    ];
    let lines: Vec<Line> = entries
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!("{key:<8}"), key_style),
                Span::styled((*label).to_string(), label_style),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines).block(block), area);
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
