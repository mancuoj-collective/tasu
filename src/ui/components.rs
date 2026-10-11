use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::domain::{Bucket, Task, TaskState};

use super::text::truncate;
use super::theme::Theme;

/// A task row: indent, marker, title. The title is truncated with an ellipsis
/// so the row (and its selection bar) always fits the available width.
pub fn task_line(task: &Task, theme: &Theme, width: u16) -> Line<'static> {
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

    // " " + mark + " "
    let available = (width as usize).saturating_sub(3);
    // A control character (a newline from `tasu add`) would count as zero width
    // and break the row's alignment; flatten it to a space.
    let title: String = task
        .title
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let title = truncate(&title, available);

    Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(title, title_style),
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

/// A task row, drawn as one highlighted bar when it is the selected row.
pub fn selectable_row(task: &Task, theme: &Theme, width: u16, selected: bool) -> Line<'static> {
    let line = task_line(task, theme, width);
    if selected {
        pad_line(line, width, theme.highlight()).style(theme.highlight())
    } else {
        line
    }
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
    use super::{Theme, task_line};
    use crate::domain::{Task, test_time::at};

    #[test]
    fn a_control_character_in_a_title_is_flattened() {
        let task = Task::new("two\nlines", at(2026, 10, 5));
        let line = task_line(&task, &Theme::DARK, 40);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            !text.contains('\n'),
            "a newline must not reach the row: {text:?}"
        );
    }
}
