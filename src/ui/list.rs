use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::app::App;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;

    if app.todos.is_empty() {
        let [_, middle, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);
        f.render_widget(
            Paragraph::new("Nothing here yet · press a to add one")
                .style(t.muted())
                .alignment(Alignment::Center),
            middle,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .todos
        .iter()
        .map(|todo| {
            let (mark, mark_style) = if todo.done {
                ("✓", Style::new().fg(t.success))
            } else {
                ("○", t.muted())
            };
            let title_style = if todo.done {
                Style::new().fg(t.fg_disabled).crossed_out()
            } else {
                t.text()
            };
            ListItem::new(Line::from(vec![
                Span::styled(mark, mark_style),
                Span::raw(" "),
                Span::styled(todo.title.as_str(), title_style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_symbol("▍ ")
        .highlight_style(t.highlight());

    let (list_area, scrollbar_area) = if app.todos.len() > area.height as usize {
        let [list, bar] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        (list, Some(bar))
    } else {
        (area, None)
    };

    f.render_stateful_widget(list, list_area, &mut app.state);

    if let Some(bar_area) = scrollbar_area {
        let mut state =
            ScrollbarState::new(app.todos.len()).position(app.state.selected().unwrap_or(0));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("┃")
            .track_style(t.border(false))
            .thumb_style(Style::new().fg(t.primary));
        f.render_stateful_widget(scrollbar, bar_area, &mut state);
    }
}
