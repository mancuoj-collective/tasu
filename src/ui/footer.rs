use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Mode};
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(hint_line(&hints(app))), area);
}

pub fn hints(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.mode {
        Mode::Normal => vec![
            ("q", "quit"),
            ("j/k", "move"),
            ("a", "add"),
            ("e", "edit"),
            ("d", "delete"),
            ("space", "toggle"),
        ],
        Mode::Add | Mode::Edit => vec![("enter", "save"), ("esc", "cancel")],
        Mode::ConfirmDelete => vec![("y", "delete"), ("n", "cancel")],
    }
}

fn hint_line(entries: &[(&'static str, &'static str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, desc)) in entries.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, theme::key()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, theme::muted()));
    }
    Line::from(spans)
}
