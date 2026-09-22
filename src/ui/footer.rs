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

pub fn hints(app: &App) -> &'static [(&'static str, &'static str)] {
    match app.mode {
        Mode::Normal => &[
            ("q", "quit"),
            ("j/k", "move"),
            ("space", "toggle"),
            ("a", "add"),
            ("e", "edit"),
            ("d", "delete"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "save"), ("esc", "cancel")],
        Mode::ConfirmDelete => &[("y", "delete"), ("n", "cancel")],
    }
}

fn hint_line(app: &App) -> Line<'static> {
    let t = app.theme;
    let (key_style, hint_style) = t.key_hint();
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, desc)) in hints(app).iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", t.disabled()));
        }
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, hint_style));
    }
    Line::from(spans)
}
