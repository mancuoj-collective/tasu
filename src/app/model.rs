use chrono::{DateTime, Local};
use tui_input::Input;

use crate::domain::{Board, Bucket};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Add,
    Edit,
    Completed,
    Help,
    Details,
}

/// Something that went wrong and stays visible until it is resolved. The footer
/// shows `summary`; the details overlay shows `detail`, which is never
/// truncated, so a long IO error or a backup path is readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub summary: String,
    pub detail: String,
}

impl Problem {
    pub fn new(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            detail: detail.into(),
        }
    }
}

/// Which list the history modal shows. Both are restorable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryView {
    Done,
    Dropped,
}

/// Background sync status, shown in the footer. A failure is not a status: it is
/// a [`Problem`], like a write error or a damaged board, so it is read the same
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// No remote configured.
    Local,
    /// Remote configured, nothing running.
    Idle,
    /// A pull or push is in flight.
    Syncing,
}

/// A transient message shown after capturing, without stealing focus.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub born: DateTime<Local>,
}

#[derive(Debug)]
pub struct UiState {
    pub mode: Mode,
    /// Index into the selectable task rows (see [`Model::rows`]).
    pub cursor: usize,
    pub input: Input,
    pub toast: Option<Toast>,
    pub done_cursor: usize,
    pub done_filter: Input,
    pub history_view: HistoryView,
    /// Background sync status, refreshed from the runtime each frame.
    pub sync: SyncStatus,
    /// The last sync failure, with the git error kept for the details overlay.
    pub sync_problem: Option<Problem>,
    /// A write error (e.g. the board could not be saved), shown in the footer
    /// until the next successful save, so data loss is never silent.
    pub error: Option<Problem>,
    /// A board file that could not be read and was moved aside. Shown in the
    /// footer until the next successful save, so a vanished board is explained.
    pub board_damage: Option<Problem>,
    /// `created_at` of the task being edited. Latched when the editor opens so a
    /// reload during editing cannot retarget the rename to a different task.
    pub edit_target: Option<DateTime<Local>>,
    /// Tick counter, used to animate the syncing spinner.
    pub tick: u64,
    /// Scroll offset of whichever overlay is open (help or details). The
    /// renderer clamps it to the content and hands the clamped value back, so
    /// key handling always starts valid.
    pub overlay_scroll: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            cursor: 0,
            input: Input::default(),
            toast: None,
            done_cursor: 0,
            done_filter: Input::default(),
            history_view: HistoryView::Done,
            sync: SyncStatus::Local,
            sync_problem: None,
            error: None,
            board_damage: None,
            edit_target: None,
            tick: 0,
            overlay_scroll: 0,
        }
    }
}

/// A rendered row: a bucket header or a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Header(Bucket),
    Task(usize),
}

#[derive(Debug)]
pub struct Model {
    pub board: Board,
    pub ui: UiState,
    pub now: DateTime<Local>,
    pub should_quit: bool,
}

impl Model {
    pub fn new(board: Board, now: DateTime<Local>) -> Self {
        Self {
            board,
            ui: UiState::default(),
            now,
            should_quit: false,
        }
    }

    pub const BUCKETS: [Bucket; 3] = [Bucket::Today, Bucket::Week, Bucket::Later];

    /// Display rows, lazily: each bucket header followed by its tasks (newest
    /// first). Yielding an iterator lets a caller that stops early — a cursor
    /// lookup, an emptiness check — avoid building buckets it never reaches.
    pub fn rows(&self) -> impl Iterator<Item = Row> + '_ {
        Self::BUCKETS.into_iter().flat_map(move |bucket| {
            std::iter::once(Row::Header(bucket))
                .chain(self.board.open_in(bucket).into_iter().map(Row::Task))
        })
    }

    /// Number of selectable (task) rows. Every open task sits in exactly one
    /// bucket, so this is a plain count with no per-bucket index list built.
    pub fn selectable_len(&self) -> usize {
        self.board
            .tasks()
            .iter()
            .filter(|task| task.is_open())
            .count()
    }

    /// The board index of the task under the cursor, if the cursor is on one.
    pub fn selected(&self) -> Option<usize> {
        self.rows()
            .filter_map(|row| match row {
                Row::Task(index) => Some(index),
                Row::Header(_) => None,
            })
            .nth(self.ui.cursor)
    }

    pub fn cursor_down(&mut self) {
        if self.ui.cursor + 1 < self.selectable_len() {
            self.ui.cursor += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        self.ui.cursor = self.ui.cursor.saturating_sub(1);
    }

    pub fn cursor_first(&mut self) {
        self.ui.cursor = 0;
    }

    pub fn cursor_last(&mut self) {
        self.ui.cursor = self.selectable_len().saturating_sub(1);
    }

    /// Move the cursor to the nearest non-empty bucket in the given direction,
    /// keeping the row offset within the bucket when possible. In the wide
    /// kanban this is literally left/right between columns.
    pub fn cursor_bucket(&mut self, delta: i32) {
        let Some((position, local)) = self.cursor_bucket_position() else {
            return;
        };
        let mut target = position as i32 + delta;
        while (0..Self::BUCKETS.len() as i32).contains(&target) {
            let count = self.board.open_count(Self::BUCKETS[target as usize]);
            if count > 0 {
                let base: usize = Self::BUCKETS[..target as usize]
                    .iter()
                    .map(|bucket| self.board.open_count(*bucket))
                    .sum();
                self.ui.cursor = base + local.min(count - 1);
                return;
            }
            target += delta;
        }
    }

    /// The cursor's `(bucket position, row within bucket)`, if any.
    fn cursor_bucket_position(&self) -> Option<(usize, usize)> {
        let mut remaining = self.ui.cursor;
        for (position, bucket) in Self::BUCKETS.iter().enumerate() {
            let len = self.board.open_count(*bucket);
            if remaining < len {
                return Some((position, remaining));
            }
            remaining -= len;
        }
        None
    }

    pub fn clamp_cursor(&mut self) {
        let len = self.selectable_len();
        if len == 0 {
            self.ui.cursor = 0;
        } else if self.ui.cursor >= len {
            self.ui.cursor = len - 1;
        }
    }

    /// Unfiltered tasks for the current history view.
    pub fn history_source(&self) -> Vec<usize> {
        match self.ui.history_view {
            HistoryView::Done => self.board.done(),
            HistoryView::Dropped => self.board.archived(),
        }
    }

    /// Tasks for the current history view matching the search (case-insensitive).
    pub fn history_items(&self) -> Vec<usize> {
        let needle = self.ui.done_filter.value().to_lowercase();
        self.history_source()
            .into_iter()
            .filter(|&index| {
                needle.is_empty()
                    || self
                        .board
                        .task(index)
                        .is_some_and(|task| task.title.to_lowercase().contains(&needle))
            })
            .collect()
    }

    /// Everything currently wrong, for the footer and the details overlay: the
    /// write error first, then board damage, then a sync failure.
    pub fn problems(&self) -> impl Iterator<Item = &Problem> {
        [
            self.ui.error.as_ref(),
            self.ui.board_damage.as_ref(),
            self.ui.sync_problem.as_ref(),
        ]
        .into_iter()
        .flatten()
    }

    pub fn set_toast(&mut self, text: impl Into<String>) {
        self.ui.toast = Some(Toast {
            text: text.into(),
            born: self.now,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    /// One open task in each bucket, so the cursor has to cross bucket
    /// boundaries: `today` (index 1), `week` (index 2), `later` (index 0).
    fn three_buckets() -> Model {
        let mut model = Model::new(Board::new(), at(2026, 10, 5));
        model.board.add("later", at(2026, 10, 4));
        model.board.add("today", at(2026, 10, 5));
        model.board.move_bucket(0, 2, at(2026, 10, 5));
        model.board.add("week", at(2026, 10, 6));
        model.board.move_bucket(2, 1, at(2026, 10, 6));
        model
    }

    #[test]
    fn rows_are_each_bucket_header_then_its_tasks() {
        let rows: Vec<Row> = three_buckets().rows().collect();
        assert_eq!(
            rows,
            vec![
                Row::Header(Bucket::Today),
                Row::Task(1),
                Row::Header(Bucket::Week),
                Row::Task(2),
                Row::Header(Bucket::Later),
                Row::Task(0),
            ]
        );
    }

    #[test]
    fn the_cursor_walks_tasks_across_buckets() {
        let mut model = three_buckets();
        assert_eq!(model.selectable_len(), 3);

        assert_eq!(model.selected(), Some(1), "today");
        model.cursor_down();
        assert_eq!(model.selected(), Some(2), "week");
        model.cursor_down();
        assert_eq!(model.selected(), Some(0), "later");
        model.cursor_down();
        assert_eq!(
            model.selected(),
            Some(0),
            "the cursor stops on the last task"
        );
    }

    #[test]
    fn problems_are_listed_in_priority_order() {
        let mut model = Model::new(Board::new(), at(2026, 10, 5));
        assert_eq!(model.problems().count(), 0);

        model.ui.error = Some(Problem::new("could not save", "could not save: nope"));
        model.ui.sync_problem = Some(Problem::new("offline", "fatal: unreachable"));
        let summaries: Vec<&str> = model.problems().map(|p| p.summary.as_str()).collect();
        assert_eq!(summaries, vec!["could not save", "offline"]);
    }

    #[test]
    fn an_empty_board_has_no_selection() {
        let model = Model::new(Board::new(), at(2026, 10, 5));
        assert_eq!(model.selectable_len(), 0);
        assert_eq!(model.selected(), None);
        assert_eq!(model.rows().count(), 3, "one header per bucket, no tasks");
    }
}
