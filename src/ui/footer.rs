use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, Mode};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(hint_line(app)), area);
}

pub fn hints(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.mode {
        Mode::Normal => vec![
            ("q", "quit"),
            ("j/k", "move"),
            ("space", "toggle"),
            ("a", "add"),
            ("e", "edit"),
            ("d", "delete"),
        ],
        Mode::Add | Mode::Edit => vec![("enter", "save"), ("esc", "cancel")],
        Mode::ConfirmDelete => vec![("y", "delete"), ("n", "cancel")],
    }
}

fn hint_line(app: &App) -> Line<'static> {
    let t = app.theme;
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, desc)) in hints(app).iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", t.disabled()));
        }
        spans.push(Span::styled(*key, t.key_hint().0));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, t.key_hint().1));
    }
    Line::from(spans)
}
