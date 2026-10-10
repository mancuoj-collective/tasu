use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::{Bucket, Task, TaskState};

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
    let title = truncate(&task.title, available);

    Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(title, title_style),
    ])
}

/// Cut `text` to at most `max` display columns, marking the cut with `…`.
/// Shared with the footer, which must truncate a possibly non-ASCII error by
/// width, not by character count.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > max - 1 {
            break;
        }
        used += width;
        out.push(ch);
    }
    out.push('\u{2026}');
    out
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
    use super::truncate;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
        assert_eq!(truncate("你好世界", 5), "你好\u{2026}");
    }

    #[test]
    fn truncation_never_exceeds_the_budget() {
        for text in ["hello world", "你好世界", "a b c d e f"] {
            for max in 1..=8 {
                assert!(
                    truncate(text, max).width() <= max,
                    "truncate({text:?}, {max}) overflowed"
                );
            }
        }
    }
}
