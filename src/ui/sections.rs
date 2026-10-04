use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{Model, Row};

use super::{components, scroll_offset, theme::Theme};

/// Vertical three-section layout: `TODAY` / `WEEK` / `LATER`, scrolled so the
/// cursor stays visible. Section structure comes from headers and spacing, not
/// boxes.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    if model.selectable_len() == 0 {
        let hint = Line::from(Span::styled("按 a 记一件事", theme.muted()));
        f.render_widget(
            Paragraph::new(hint).centered(),
            Rect {
                y: area.y + area.height / 2,
                height: 1,
                ..area
            },
        );
        return;
    }

    let rows = model.rows();
    let height = area.height as usize;
    let offset = scroll_offset(model.cursor_row(), rows.len(), height);

    let mut lines: Vec<Line> = Vec::with_capacity(rows.len());
    let mut selectable = 0usize;
    for row in &rows {
        match row {
            Row::Header(bucket) => lines.push(components::header_line(*bucket, theme, model.now)),
            Row::Task(index) => {
                let Some(task) = model.board.task(*index) else {
                    continue;
                };
                let mut line = components::task_line(task, theme, model.now);
                if selectable == model.ui.cursor {
                    line = line.style(theme.highlight());
                }
                lines.push(line);
                selectable += 1;
            }
        }
    }

    let visible: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();
    f.render_widget(Paragraph::new(visible), area);
}
