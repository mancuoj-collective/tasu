use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{
        Block, BorderType, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};

use crate::{
    app::{App, Mode},
    theme::Theme,
    todo::Todo,
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    if app.todos.is_empty() {
        empty(f, t, area);
        return;
    }

    let done = app.todos.done_count();
    let total = app.todos.len();

    let title = Line::from(vec![
        Span::styled(format!(" {} ", env!("CARGO_PKG_NAME")), t.title()),
        Span::styled(format!("· {done}/{total} "), t.muted()),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(t.border(app.mode == Mode::Normal))
        .padding(Padding::new(1, 1, 1, 1))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let position = app.todos.selected().unwrap_or(0);
    let (todos, state) = app.todos.split_mut();

    let items: Vec<ListItem> = todos.iter().map(|todo| list_item(todo, t)).collect();
    let list = List::new(items)
        .highlight_symbol("▍ ")
        .highlight_style(t.highlight());

    let (list_area, scrollbar_area) = if total > inner.height as usize {
        let [list, bar] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        (list, Some(bar))
    } else {
        (inner, None)
    };

    f.render_stateful_widget(list, list_area, state);

    if let Some(bar_area) = scrollbar_area {
        let mut state = ScrollbarState::new(total).position(position);
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

fn empty(f: &mut Frame, t: Theme, area: Rect) {
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    let (key_style, hint_style) = t.key_hint();
    let hint = Line::from(vec![
        Span::styled("Nothing here yet · press ", hint_style),
        Span::styled("a", key_style),
        Span::styled(" to add one", hint_style),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), middle);
}

fn list_item<'a>(todo: &'a Todo, t: Theme) -> ListItem<'a> {
    let (mark, mark_style) = if todo.done {
        ("✓", Style::new().fg(t.success))
    } else {
        ("○", t.muted())
    };
    let title_style = if todo.done {
        t.disabled().crossed_out()
    } else {
        t.text()
    };
    ListItem::new(Line::from(vec![
        Span::styled(mark, mark_style),
        Span::raw(" "),
        Span::styled(todo.title.as_str(), title_style),
    ]))
}
