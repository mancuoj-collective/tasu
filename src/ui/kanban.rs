use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::Model;
use crate::domain::Bucket;

use super::{components, scroll_offset, theme::Theme};

/// Wide layout: the three buckets side by side, separated by vertical rules.
/// Task titles are read across, so this only kicks in when there is genuinely
/// room for three columns.
pub fn draw(f: &mut Frame, model: &Model, area: Rect, theme: &Theme) {
    let [today, divider_a, week, divider_b, later] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
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

    for divider in [divider_a, divider_b] {
        draw_divider(f, divider, theme);
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
    let [title, rule] = components::header_lines(bucket, theme, model.now, area.width);
    let mut lines: Vec<Line> = vec![title, rule];

    let body_height = area.height.saturating_sub(lines.len() as u16) as usize;
    let local_selected = model
        .ui
        .cursor
        .checked_sub(base)
        .filter(|&local| local < indices.len());
    let offset = local_selected
        .map(|local| scroll_offset(local, indices.len(), body_height))
        .unwrap_or(0);

    for (position, &index) in indices.iter().enumerate().skip(offset).take(body_height) {
        let Some(task) = model.board.task(index) else {
            continue;
        };
        let mut line = components::task_line(task, theme, area.width);
        if Some(position) == local_selected {
            line = components::pad_line(line, area.width, theme.highlight());
            line = line.style(theme.highlight());
        }
        lines.push(line);
    }

    if hidden > 0 {
        lines.push(components::fold_line(hidden, theme));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_divider(f: &mut Frame, area: Rect, theme: &Theme) {
    let lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled(" \u{2502} ", theme.disabled())))
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}
