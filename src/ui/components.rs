use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::text::{Line, Span};

use crate::domain::{Bucket, Task, TaskState};

use super::theme::Theme;

/// A task row: marker, title, and — for stale open tasks — how long it has been
/// carried. The badge turns to the warning color at three days.
pub fn task_line(task: &Task, theme: &Theme, now: DateTime<Local>) -> Line<'static> {
    let (mark, mark_style) = match task.state {
        TaskState::Done => ("\u{2713}", theme.success()),
        _ => ("\u{25cb}", theme.muted()),
    };
    let title_style = if task.state == TaskState::Open {
        theme.text()
    } else {
        theme.disabled().crossed_out()
    };

    let mut spans = vec![
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(task.title.clone(), title_style),
    ];

    if task.is_open() && task.bucket != Bucket::Later {
        let days = (now.date_naive() - task.bucket_since.date_naive()).num_days();
        if days >= 1 {
            let style = if days >= 3 {
                theme.warn()
            } else {
                theme.muted()
            };
            spans.push(Span::styled(format!("  \u{b7} 顺延 {days} 天"), style));
        }
    }

    Line::from(spans)
}

pub fn header_line(bucket: Bucket, theme: &Theme, now: DateTime<Local>) -> Line<'static> {
    match bucket {
        Bucket::Today => Line::from(Span::styled("TODAY", theme.today())),
        Bucket::Later => Line::from(Span::styled("LATER", theme.accent())),
        Bucket::Week => Line::from(vec![
            Span::styled("WEEK  ", theme.accent()),
            Span::styled(week_meta(now), theme.muted()),
        ]),
    }
}

/// The collapsed tail of `Later`: `⋯ 更早的 12 条（l 展开）`.
pub fn fold_line(hidden: usize, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("\u{22ef} 更早的 {hidden} 条（l 展开）"),
        theme.muted(),
    ))
}

/// `40 · 今年还剩 13 周`.
fn week_meta(now: DateTime<Local>) -> String {
    let date = now.date_naive();
    let current = date.iso_week();
    let last = NaiveDate::from_ymd_opt(date.year(), 12, 28)
        .map(|d| d.iso_week().week())
        .unwrap_or(current.week());
    let remaining = last.saturating_sub(current.week());
    format!("{} · 今年还剩 {} 周", current.week(), remaining)
}
