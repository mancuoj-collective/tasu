use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme;

pub fn draw(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!(" {} ", env!("CARGO_PKG_NAME")), theme::title()),
        Span::styled(format!(" v{}", env!("CARGO_PKG_VERSION")), theme::muted()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
