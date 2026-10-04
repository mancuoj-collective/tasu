use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::domain::{Bucket, Task, TaskState};

use super::theme::Theme;

/// A task row: indent, marker, title.
pub fn task_line(task: &Task, theme: &Theme) -> Line<'static> {
    let (mark, mark_style) = match task.state {
        TaskState::Done => ("\u{2713}", theme.success()),
        TaskState::Archived => ("\u{2717}", theme.muted()),
        TaskState::Open => ("\u{25cb}", theme.muted()),
    };
    let title_style = match task.state {
        TaskState::Open => theme.text(),
        TaskState::Done => theme.disabled().crossed_out(),
        TaskState::Archived => theme.disabled(),
    };

    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(task.title.clone(), title_style),
    ])
}

/// Two lines that open a section: a label (with the ISO week for `THIS WEEK`),
/// then a full-width rule. The rule is what makes the buckets legible.
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
            Span::styled("THIS WEEK", theme.accent()),
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
