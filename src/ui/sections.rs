use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::Model;

use super::{components, scroll_offset, scrollbar, theme::Theme};

/// Vertical three-section layout: `TODAY` / `THIS WEEK` / `LATER`. Each section
/// is a labeled rule line followed by its tasks, separated by a blank line, and
/// scrolled so the cursor stays visible. A thin scrollbar appears on overflow.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    if model.selectable_len() == 0 {
        let hint = Line::from(Span::styled("press a to add something", theme.muted()));
        f.render_widget(Paragraph::new(hint).centered(), center_row(area));
        return;
    }

    let height = area.height as usize;
    let overflow = line_count(model) > height && area.width > 1;
    let width = area.width.saturating_sub(u16::from(overflow));

    let mut lines: Vec<Line> = Vec::new();
    let mut selectable = 0usize;
    let mut selected_line = 0usize;

    for (position, bucket) in Model::BUCKETS.iter().enumerate() {
        let (indices, hidden) = model.bucket_view(*bucket);
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
            let mut line = components::task_line(task, theme, width);
            if selectable == model.ui.cursor {
                selected_line = lines.len();
                line = components::pad_line(line, width, theme.highlight());
                line = line.style(theme.highlight());
            }
            lines.push(line);
            selectable += 1;
        }

        if hidden > 0 {
            lines.push(components::fold_line(hidden, theme));
        }
    }

    let offset = scroll_offset(selected_line, lines.len(), height);
    let visible: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();

    let (text_area, bar_area) = if overflow {
        let [text, bar] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        (text, Some(bar))
    } else {
        (area, None)
    };

    f.render_widget(Paragraph::new(visible), text_area);
    if let Some(bar) = bar_area {
        scrollbar(f, bar, line_count(model), offset, height, theme);
    }
}

/// Number of display lines the sections will occupy, for overflow detection.
fn line_count(model: &Model) -> usize {
    let mut count = 0;
    for (position, bucket) in Model::BUCKETS.iter().enumerate() {
        let (indices, hidden) = model.bucket_view(*bucket);
        if position > 0 {
            count += 1;
        }
        count += 2 + indices.len();
        if hidden > 0 {
            count += 1;
        }
    }
    count
}

fn center_row(area: Rect) -> Rect {
    Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
    }
}
