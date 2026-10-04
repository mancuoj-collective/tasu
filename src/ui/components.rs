use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::domain::{Bucket, Task, TaskState};

use super::theme::Theme;

/// A task row: indent, marker, title, and — for stale open tasks — how long it
/// has been carried. The badge turns to the warning color at three days.
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
        Span::raw("  "),
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(task.title.clone(), title_style),
    ];

    // The carry badge only makes sense on `Week`: `settle` guarantees a `Today`
    // task is fresh, and `Later` is already old.
    if task.is_open() && task.bucket == Bucket::Week {
        let days = (now.date_naive() - task.bucket_since.date_naive()).num_days();
        if days >= 1 {
            let style = if days >= 3 {
                theme.warn()
            } else {
                theme.muted()
            };
            spans.push(Span::styled(format!("  \u{b7} {days}d"), style));
        }
    }

    Line::from(spans)
}

/// Two lines that open a section: a label (with the ISO week for `WEEK`), then
/// a full-width rule. The rule is what makes the buckets legible at a glance.
pub fn header_lines(
    bucket: Bucket,
    theme: &Theme,
    now: DateTime<Local>,
    width: u16,
) -> [Line<'static>; 2] {
    let title = match bucket {
        Bucket::Today => Line::from(Span::styled("TODAY", theme.today())),
        Bucket::Later => Line::from(Span::styled("LATER", theme.accent())),
        Bucket::Week => Line::from(vec![
            Span::styled("WEEK", theme.accent()),
            Span::styled(format!("  {}", week_meta(now)), theme.muted()),
        ]),
    };
    let rule = Line::from(Span::styled(
        "\u{2500}".repeat(width as usize),
        theme.disabled(),
    ));
    [title, rule]
}

/// The collapsed tail of `Later`: `⋯ 12 more (l to expand)`.
pub fn fold_line(hidden: usize, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  \u{22ef} {hidden} more (l to expand)"),
        theme.muted(),
    ))
}

/// Extend a line to the full width with styled blanks, so a highlighted row
/// reads as one continuous bar instead of stopping at the last character.
pub fn pad_line(mut line: Line<'static>, width: u16, style: Style) -> Line<'static> {
    let used: usize = line.spans.iter().map(|span| span.content.width()).sum();
    let padding = (width as usize).saturating_sub(used);
    if padding > 0 {
        line.spans.push(Span::styled(" ".repeat(padding), style));
    }
    line
}

/// `40/53`: ISO week number out of the total weeks in the year.
fn week_meta(now: DateTime<Local>) -> String {
    let date = now.date_naive();
    let current = date.iso_week().week();
    let last = NaiveDate::from_ymd_opt(date.year(), 12, 28)
        .map(|day| day.iso_week().week())
        .unwrap_or(current);
    format!("{current}/{last}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(day: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
            .single()
            .unwrap()
    }

    fn text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn carry_badge_appears_only_on_week() {
        let now = dt(8);

        let today = Task::new("fresh", dt(7));
        assert!(!text(&task_line(&today, &Theme::DARK, now)).contains("1d"));

        let mut week = Task::new("slipped", dt(7));
        week.bucket = Bucket::Week;
        assert!(text(&task_line(&week, &Theme::DARK, now)).contains("1d"));
    }
}
