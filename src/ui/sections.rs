use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::Model;

use super::{components, scroll_offset, theme::Theme};

/// Vertical three-section layout: `TODAY` / `WEEK` / `LATER`. Each section is a
/// labeled rule line followed by its tasks, separated by a blank line, and
/// scrolled so the cursor stays visible.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    if model.selectable_len() == 0 {
        let hint = Line::from(Span::styled("press a to add something", theme.muted()));
        f.render_widget(Paragraph::new(hint).centered(), center_row(area));
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut selectable = 0usize;
    let mut selected_line = 0usize;

    for (position, bucket) in Model::BUCKETS.iter().enumerate() {
        let (indices, hidden) = model.bucket_view(*bucket);
        if position > 0 {
            lines.push(Line::default());
        }
        let [title, rule] = components::header_lines(*bucket, theme, model.now, area.width);
        lines.push(title);
        lines.push(rule);

        for &index in &indices {
            let Some(task) = model.board.task(index) else {
                continue;
            };
            let mut line = components::task_line(task, theme, area.width);
            if selectable == model.ui.cursor {
                selected_line = lines.len();
                line = components::pad_line(line, area.width, theme.highlight());
                line = line.style(theme.highlight());
            }
            lines.push(line);
            selectable += 1;
        }

        if hidden > 0 {
            lines.push(components::fold_line(hidden, theme));
        }
    }

    let height = area.height as usize;
    let offset = scroll_offset(selected_line, lines.len(), height);
    let visible: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();
    f.render_widget(Paragraph::new(visible), area);
}

fn center_row(area: Rect) -> Rect {
    Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
    }
}
