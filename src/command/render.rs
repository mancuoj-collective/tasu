//! Plain-text rendering of the board for the one-shot commands. Pure: every
//! function takes the data and returns the text.

use crate::domain::{Board, Bucket, Task};

use super::HistoryList;

/// One history section: whether it is the dropped list, and its tasks, newest
/// first. Named fields instead of a bare `(bool, Vec<_>)` tuple, so the flag
/// cannot be transposed.
pub(super) struct HistorySection<'a> {
    pub dropped: bool,
    pub tasks: Vec<&'a Task>,
}

/// The requested history views, in print order. Empty views are kept so
/// `render_history` can skip them.
pub(super) fn history_sections(board: &Board, view: HistoryList) -> Vec<HistorySection<'_>> {
    let wanted: &[bool] = match view {
        HistoryList::Both => &[false, true],
        HistoryList::Done => &[false],
        HistoryList::Dropped => &[true],
    };
    wanted
        .iter()
        .map(|&dropped| {
            let indices = if dropped {
                board.archived()
            } else {
                board.done()
            };
            let tasks = indices
                .iter()
                .filter_map(|&index| board.task(index))
                .collect();
            HistorySection { dropped, tasks }
        })
        .collect()
}

pub(super) fn render_history(sections: &[HistorySection<'_>], view: HistoryList) -> String {
    let mut rendered: Vec<String> = Vec::new();
    for section in sections {
        if section.tasks.is_empty() {
            continue;
        }
        let dropped = section.dropped;
        let mark = if dropped { '\u{2717}' } else { '\u{2713}' };
        let mut out = String::new();
        out.push_str(if dropped { "DROPPED" } else { "DONE" });
        out.push('\n');
        for task in &section.tasks {
            out.push_str("  ");
            out.push(mark);
            out.push(' ');
            out.push_str(&task.title);
            out.push('\n');
        }
        rendered.push(out);
    }
    if rendered.is_empty() {
        match view {
            HistoryList::Both => "nothing done or dropped\n".to_string(),
            HistoryList::Done => "nothing done\n".to_string(),
            HistoryList::Dropped => "nothing dropped\n".to_string(),
        }
    } else {
        rendered.join("\n")
    }
}

pub(super) fn render_open(board: &Board, buckets: &[Bucket]) -> String {
    let mut out = String::new();
    for bucket in buckets {
        let indices = board.open_in(*bucket);
        if indices.is_empty() {
            continue;
        }
        out.push_str(bucket_label(*bucket));
        out.push('\n');
        for index in indices {
            if let Some(task) = board.task(index) {
                out.push_str("  \u{25cb} ");
                out.push_str(&task.title);
                out.push('\n');
            }
        }
    }
    if out.is_empty() {
        out.push_str("no open tasks\n");
    }
    out
}

pub(super) fn bucket_label(bucket: Bucket) -> &'static str {
    match bucket {
        Bucket::Today => "TODAY",
        Bucket::Week => "THIS WEEK",
        Bucket::Later => "LATER",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    #[test]
    fn list_prints_open_tasks_by_bucket() {
        let mut board = Board::new();
        board.add("today one", at(2026, 10, 5));
        board.add("later one", at(2026, 10, 5));
        board.move_bucket(1, 2, at(2026, 10, 5));

        assert_eq!(
            render_open(&board, &[Bucket::Today, Bucket::Week, Bucket::Later]),
            "TODAY\n  \u{25cb} today one\nLATER\n  \u{25cb} later one\n"
        );
    }

    #[test]
    fn list_reports_an_empty_board() {
        assert_eq!(
            render_open(&Board::new(), &[Bucket::Today]),
            "no open tasks\n"
        );
    }

    #[test]
    fn history_shows_both_views_by_default() {
        let mut board = Board::new();
        board.add("done thing", at(2026, 10, 5));
        board.add("dropped thing", at(2026, 10, 5));
        board.complete(0, at(2026, 10, 5));
        board.archive(1, at(2026, 10, 5));

        let sections = history_sections(&board, HistoryList::Both);
        assert_eq!(
            render_history(&sections, HistoryList::Both),
            "DONE\n  \u{2713} done thing\n\nDROPPED\n  \u{2717} dropped thing\n"
        );
    }

    #[test]
    fn history_can_show_one_view() {
        let mut board = Board::new();
        board.add("done thing", at(2026, 10, 5));
        board.add("dropped thing", at(2026, 10, 5));
        board.complete(0, at(2026, 10, 5));
        board.archive(1, at(2026, 10, 5));

        let sections = history_sections(&board, HistoryList::Dropped);
        assert_eq!(
            render_history(&sections, HistoryList::Dropped),
            "DROPPED\n  \u{2717} dropped thing\n"
        );
    }
}
