use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::Paragraph,
};

use crate::app::Model;
use crate::domain::Bucket;

use super::{components, scroll_offset, theme::Theme};

/// Wide layout: the three buckets side by side. Task titles are read across, so
/// this only kicks in when there is genuinely room for three columns.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    let [today, _, week, _, later] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(area);

    let mut base = 0usize;
    for (rect, bucket) in [
        (today, Bucket::Today),
        (week, Bucket::Week),
        (later, Bucket::Later),
    ] {
        let (indices, hidden) = model.bucket_view(bucket);
        draw_column(f, model, rect, bucket, &indices, hidden, base, theme);
        base += indices.len();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_column(
    f: &mut Frame,
    model: &Model,
    area: Rect,
    bucket: Bucket,
    indices: &[usize],
    hidden: usize,
    base: usize,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = vec![
        components::header_line(bucket, theme, model.now),
        Line::default(),
    ];

    let height = area.height.saturating_sub(lines.len() as u16) as usize;
    let local_selected = model
        .ui
        .cursor
        .checked_sub(base)
        .filter(|&local| local < indices.len());
    let offset = local_selected
        .map(|local| scroll_offset(local, indices.len(), height))
        .unwrap_or(0);

    for (position, &index) in indices.iter().enumerate().skip(offset).take(height) {
        let Some(task) = model.board.task(index) else {
            continue;
        };
        let mut line = components::task_line(task, theme, model.now);
        if Some(position) == local_selected {
            line = line.style(theme.highlight());
        }
        lines.push(line);
    }

    if hidden > 0 {
        lines.push(components::fold_line(hidden, theme));
    }

    f.render_widget(Paragraph::new(lines), area);
}
