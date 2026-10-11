//! Plain-text layout for the UI: wrapping and truncation to display cells.
//! Pure: every function takes text and a width and returns the pieces.

use ratatui::{
    layout::Alignment,
    style::Style,
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Greedy wrap to at most `width` display cells. Break opportunities are
/// whitespace and `/`, so paths and URLs break at their own separators and the
/// first line fills up, leaving a shorter remainder (long-top, short-bottom).
/// An atom wider than a line is hard-broken character by character. The text is
/// preserved exactly. Always returns at least one line.
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out: Vec<String> = Vec::new();

    for paragraph in text.split('\n') {
        let mut line = String::new();
        // Display width of `line`, tracked incrementally instead of re-measuring
        // the growing string for every atom: `full` counts trailing spaces,
        // `trimmed` does not.
        let mut full = 0usize;
        let mut trimmed = 0usize;
        for atom in atoms(paragraph) {
            // Appending a non-blank atom makes the line's trailing spaces count;
            // appending only blanks leaves the trimmed width alone.
            let atom_content = atom.trim_end().width();
            let combined = if atom_content == 0 {
                trimmed
            } else {
                full + atom_content
            };
            if !line.is_empty() && combined > width {
                out.push(std::mem::take(&mut line));
                full = 0;
                trimmed = 0;
            }
            let mut rest = atom.as_str();
            while !rest.is_empty() {
                let room = width.saturating_sub(trimmed);
                if room == 0 {
                    out.push(std::mem::take(&mut line));
                    full = 0;
                    trimmed = 0;
                    continue;
                }
                let (chunk, tail) = split_at_width(rest, room);
                let chunk_width = chunk.width();
                let chunk_trim = chunk.trim_end().width();
                full += chunk_width;
                // A chunk with content turns any previous trailing spaces into
                // interior ones, so the trimmed width is the new full width
                // minus this chunk's own trailing spaces.
                if chunk_trim > 0 {
                    trimmed = full - (chunk_width - chunk_trim);
                }
                line.push_str(&chunk);
                rest = tail;
                if !rest.is_empty() {
                    // The chunk filled the line; flush before continuing.
                    out.push(std::mem::take(&mut line));
                    full = 0;
                    trimmed = 0;
                }
            }
        }
        out.push(line);
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Split into atoms at whitespace and `/`, keeping the separator on the atom it
/// follows so the pieces reassemble exactly.
fn atoms(text: &str) -> Vec<String> {
    let mut atoms = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if ch == ' ' || ch == '/' {
            atoms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        atoms.push(current);
    }
    atoms
}

/// Split `text` at the widest prefix that fits in `width` cells. Takes one
/// character even if it overflows, so the caller always makes progress.
fn split_at_width(text: &str, width: usize) -> (String, &str) {
    let mut used = 0usize;
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > width {
            break;
        }
        used += cells;
        end = index + ch.len_utf8();
    }
    if end == 0 && !text.is_empty() {
        end = text.chars().next().map(char::len_utf8).unwrap_or(0);
    }
    (text[..end].to_string(), &text[end..])
}

/// Cut `text` to at most `max` display columns, marking the cut with `…`.
/// Shared by the task rows and the footer, which must truncate a possibly
/// non-ASCII string by width, not by character count.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    // An ellipsis needs a cell of its own; with no room, return nothing rather
    // than a one-cell `…` that overflows the budget.
    if max == 0 {
        return String::new();
    }
    if max == 1 {
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

/// Append a labelled, wrapped block to `lines`. The first line begins with
/// `prefix`; continuation lines are indented under it. Long tokens without
/// spaces (paths, URLs) are hard-broken, so nothing is ever cut off.
pub(super) fn push_block(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    body: &str,
    style: Style,
    inner: usize,
    align: Alignment,
) {
    let indent = prefix.width();
    let body_width = inner.saturating_sub(indent).max(1);
    for (i, chunk) in wrap_text(body, body_width).into_iter().enumerate() {
        let chunk = chunk.trim_end();
        let lead = if i == 0 {
            prefix.to_string()
        } else {
            " ".repeat(indent)
        };
        let pad = match align {
            Alignment::Right => body_width.saturating_sub(chunk.width()),
            Alignment::Center => body_width.saturating_sub(chunk.width()) / 2,
            Alignment::Left => 0,
        };
        lines.push(Line::from(Span::styled(
            format!("{lead}{}{chunk}", " ".repeat(pad)),
            style,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::{truncate, wrap_text};
    use unicode_width::UnicodeWidthStr;

    fn widths(lines: &[String]) -> Vec<usize> {
        lines.iter().map(|line| line.trim_end().width()).collect()
    }

    fn trimmed(lines: &[String]) -> Vec<&str> {
        lines.iter().map(|line| line.trim_end()).collect()
    }

    #[test]
    fn short_text_is_a_single_line() {
        assert_eq!(wrap_text("a/b/c", 20), vec!["a/b/c".to_string()]);
    }

    #[test]
    fn words_wrap_on_spaces_without_losing_text() {
        let text = "fatal: repository not found";
        let lines = wrap_text(text, 12);
        assert!(lines.len() > 1, "should wrap: {lines:?}");
        assert!(widths(&lines).iter().all(|&w| w <= 12), "{lines:?}");
        assert_eq!(lines.concat(), text);
    }

    #[test]
    fn paths_fill_the_first_line_and_leave_a_short_remainder() {
        let lines = wrap_text("~/Library/Application Support/tasu/todos.json", 44);
        assert_eq!(
            trimmed(&lines),
            vec!["~/Library/Application Support/tasu/", "todos.json"]
        );
    }

    #[test]
    fn urls_break_after_a_slash() {
        let lines = wrap_text("https://github.com/mancuoj-collective/tasu-data.git", 44);
        assert_eq!(
            trimmed(&lines),
            vec!["https://github.com/mancuoj-collective/", "tasu-data.git"]
        );
    }

    #[test]
    fn long_tokens_are_hard_broken_without_loss() {
        let token = "https://github.com/mancuoj-collective/tasu-data.git";
        let lines = wrap_text(token, 24);
        assert!(lines.len() > 1, "should wrap: {lines:?}");
        assert!(widths(&lines).iter().all(|&w| w <= 24), "{lines:?}");
        assert_eq!(lines.concat(), token);
    }

    #[test]
    fn wrapping_counts_display_cells() {
        // CJK characters occupy two cells each.
        let lines = wrap_text("数据数据数据", 5);
        assert!(widths(&lines).iter().all(|&w| w <= 5), "{lines:?}");
        assert_eq!(lines.concat(), "数据数据数据");
    }

    #[test]
    fn empty_text_still_yields_one_line() {
        assert_eq!(wrap_text("", 10), vec![String::new()]);
    }

    #[test]
    fn a_trailing_space_counts_when_the_next_word_would_overflow() {
        // "foo bar" is 7 cells; at width 6 it must wrap even though the
        // non-blank widths (3 + 3) would fit, because the space counts.
        let lines = wrap_text("foo bar", 6);
        assert_eq!(trimmed(&lines), vec!["foo", "bar"]);
        assert!(widths(&lines).iter().all(|&w| w <= 6), "{lines:?}");
    }

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
            for max in 0..=8 {
                assert!(
                    truncate(text, max).width() <= max,
                    "truncate({text:?}, {max}) overflowed"
                );
            }
        }
    }
}
