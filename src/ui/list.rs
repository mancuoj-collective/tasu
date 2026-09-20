use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, List, ListItem, Paragraph},
};

use crate::{app::App, theme};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.todos.is_empty() {
        f.render_widget(
            Paragraph::new(" nothing here yet — press a to add one").style(theme::muted()),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .todos
        .iter()
        .map(|todo| {
            let mark = if todo.done { "[x]" } else { "[ ]" };
            let style = if todo.done {
                theme::muted()
            } else {
                Style::new()
            };
            ListItem::new(format!(" {mark} {}", todo.title)).style(style)
        })
        .collect();

    let list = List::new(items).highlight_style(theme::selected()).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::border()),
    );

    f.render_stateful_widget(list, area, &mut app.state);
}
