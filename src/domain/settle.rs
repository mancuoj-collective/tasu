use chrono::{DateTime, Datelike, Local};

use super::board::Board;
use super::task::{Bucket, TaskState};

/// Advance the aging pipeline to `now`.
///
/// - A `Today` task whose `bucket_since` is on an earlier day becomes `Week`.
/// - A `Week` task whose `bucket_since` is in an earlier ISO week becomes `Later`.
///
/// Automatic demotion never touches `bucket_since`, so a task left stale for
/// longer than a week cascades straight from `Today` to `Later` in one pass.
/// Idempotent for a fixed `now`. Returns whether anything moved.
pub fn settle(board: &mut Board, now: DateTime<Local>) -> bool {
    let today = now.date_naive();
    let week = iso_week(now);
    let mut changed = false;

    for task in board.tasks_mut() {
        if task.state != TaskState::Open {
            continue;
        }
        if task.bucket == Bucket::Today && task.bucket_since.date_naive() < today {
            task.bucket = Bucket::Week;
            changed = true;
        }
        if task.bucket == Bucket::Week && iso_week(task.bucket_since) < week {
            task.bucket = Bucket::Later;
            changed = true;
        }
    }

    changed
}

/// ISO year and week, so weeks spanning a year boundary compare correctly.
fn iso_week(dt: DateTime<Local>) -> (i32, u32) {
    let iso = dt.date_naive().iso_week();
    (iso.year(), iso.week())
}

#[cfg(test)]
mod tests {
    use super::settle;
    use crate::domain::test_time::at;
    use crate::domain::{Board, Bucket, Task, TaskState};

    #[test]
    fn keeps_today_within_the_same_day() {
        let mut board = Board::new();
        board.add("write tests", at(2026, 10, 5));
        assert!(!settle(&mut board, at(2026, 10, 5)));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Today);
    }

    #[test]
    fn demotes_overdue_today_to_week() {
        let mut board = Board::new();
        board.add("write tests", at(2026, 10, 5));
        assert!(settle(&mut board, at(2026, 10, 6)));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Week);
    }

    #[test]
    fn demotes_week_to_later_across_iso_week() {
        let mut board = Board::from_tasks(vec![Task::new("shipped late", at(2026, 10, 5))]);
        board.move_bucket(0, 1, at(2026, 10, 5));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Week);
        assert!(settle(&mut board, at(2026, 10, 12)));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Later);
    }

    #[test]
    fn cascades_stale_today_straight_to_later() {
        let mut board = Board::new();
        board.add("forgotten", at(2026, 10, 5));
        settle(&mut board, at(2026, 10, 14));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Later);
    }

    #[test]
    fn is_idempotent() {
        let mut board = Board::new();
        board.add("write tests", at(2026, 10, 5));
        assert!(settle(&mut board, at(2026, 10, 6)));
        assert!(!settle(&mut board, at(2026, 10, 6)));
    }

    #[test]
    fn never_ages_done_or_archived() {
        let mut board = Board::new();
        board.add("done thing", at(2026, 10, 5));
        board.add("dropped thing", at(2026, 10, 5));
        board.complete(0, at(2026, 10, 5));
        board.archive(1, at(2026, 10, 5));

        settle(&mut board, at(2026, 11, 30));

        assert_eq!(board.task(0).unwrap().state, TaskState::Done);
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Today);
        assert_eq!(board.task(1).unwrap().state, TaskState::Archived);
        assert_eq!(board.task(1).unwrap().bucket, Bucket::Today);
    }
}
