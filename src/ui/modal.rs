use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{App, Mode};
use crate::theme;

pub fn draw(f: &mut Frame, app: &App) {
    match app.mode {
        Mode::Add | Mode::Edit => input_modal(f, app),
        Mode::ConfirmDelete => confirm_modal(f, app),
        Mode::Normal => {}
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}

fn input_modal(f: &mut Frame, app: &App) {
    let title = if app.mode == Mode::Add {
        " add "
    } else {
        " edit "
    };
    let area = centered(f.area(), 50, 3);

    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(theme::border())
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, theme::border()));
    let inner = block.inner(area);
    f.render_widget(
        Paragraph::new(app.input.as_str())
            .style(theme::text())
            .block(block),
        area,
    );

    f.set_cursor_position((inner.x + app.input.chars().count() as u16, inner.y));
}

fn confirm_modal(f: &mut Frame, app: &App) {
    let area = centered(f.area(), 40, 3);
    f.render_widget(Clear, area);

    let msg = match app.state.selected() {
        Some(i) => Line::from(vec![
            Span::styled(" delete ", theme::text()),
            Span::styled(format!("\"{}\"", app.todos[i].title), theme::danger()),
            Span::styled("? ", theme::text()),
        ]),
        None => Line::from(" nothing selected ").style(theme::muted()),
    };
    let block = Block::bordered()
        .border_style(theme::border())
        .padding(Padding::horizontal(1))
        .title(Span::styled(" confirm ", theme::border()));
    f.render_widget(Paragraph::new(msg).block(block), area);
}
