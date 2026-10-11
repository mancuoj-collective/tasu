//! Plain-text wrapping for the overlays. Pure: every function takes text and a
//! width in display cells and returns the pieces.

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
        for atom in atoms(paragraph) {
            if !line.is_empty() && format!("{line}{atom}").trim_end().width() > width {
                out.push(std::mem::take(&mut line));
            }
            let mut rest = atom.as_str();
            while !rest.is_empty() {
                let room = width.saturating_sub(line.trim_end().width());
                if room == 0 {
                    out.push(std::mem::take(&mut line));
                    continue;
                }
                let (chunk, tail) = split_at_width(rest, room);
                line.push_str(&chunk);
                rest = tail;
                if !rest.is_empty() {
                    // The chunk filled the line; flush before continuing.
                    out.push(std::mem::take(&mut line));
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

#[cfg(test)]
mod tests {
    use super::wrap_text;
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
}
