use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::app::Model;

use super::{components, scroll_offset, theme::Theme};

/// Vertical three-section layout: `TODAY` / `THIS WEEK` / `LATER`. Each section
/// is a labeled rule line followed by its tasks, separated by a blank line, and
/// scrolled so the cursor stays visible.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    let width = area.width;
    let mut lines: Vec<Line> = Vec::new();
    let mut selectable = 0usize;
    let mut selected_line = 0usize;

    for (position, bucket) in Model::BUCKETS.iter().enumerate() {
        let indices = model.board.open_in(*bucket);
        if position > 0 {
            lines.push(Line::default());
        }
        let [title, rule] = components::header_lines(*bucket, theme, model.now, width);
        lines.push(title);
        lines.push(rule);

        for &index in &indices {
            let Some(task) = model.board.task(index) else {
                continue;
            };
            let selected = selectable == model.ui.cursor;
            if selected {
                selected_line = lines.len();
            }
            lines.push(components::selectable_row(task, theme, width, selected));
            selectable += 1;
        }
    }

    let height = area.height as usize;
    let offset = scroll_offset(selected_line, lines.len(), height);
    let visible: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();
    f.render_widget(Paragraph::new(visible), area);
}
