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
    pub later_expanded: bool,
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
            later_expanded: false,
        }
    }
}

/// A rendered row: a bucket header, a task, or the folded tail of `Later`.
#[derive(Debug, Clone, Copy)]
pub enum Row {
    Header(Bucket),
    Task(usize),
    Fold(usize),
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

    /// Collapsed `Later` shows at most this many recent tasks.
    pub const LATER_VISIBLE: usize = 5;

    /// Visible task indices for a bucket, plus how many older `Later` tasks are
    /// folded away. `rows` and the kanban both build from this so they agree.
    pub fn bucket_view(&self, bucket: Bucket) -> (Vec<usize>, usize) {
        let mut indices = self.board.open_in(bucket);
        if bucket == Bucket::Later && !self.ui.later_expanded && indices.len() > Self::LATER_VISIBLE
        {
            let hidden = indices.len() - Self::LATER_VISIBLE;
            indices.truncate(Self::LATER_VISIBLE);
            (indices, hidden)
        } else {
            (indices, 0)
        }
    }

    /// Display rows: each bucket header followed by its tasks (newest first).
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for bucket in Self::BUCKETS {
            rows.push(Row::Header(bucket));
            let (indices, hidden) = self.bucket_view(bucket);
            rows.extend(indices.into_iter().map(Row::Task));
            if hidden > 0 {
                rows.push(Row::Fold(hidden));
            }
        }
        rows
    }

    /// Number of selectable (task) rows.
    pub fn selectable_len(&self) -> usize {
        self.rows()
            .iter()
            .filter(|row| matches!(row, Row::Task(_)))
            .count()
    }

    pub fn selected(&self) -> Option<usize> {
        let mut seen = 0;
        for row in self.rows() {
            if let Row::Task(index) = row {
                if seen == self.ui.cursor {
                    return Some(index);
                }
                seen += 1;
            }
        }
        None
    }

    /// Position of the cursor in [`Model::rows`], for scrolling.
    pub fn cursor_row(&self) -> usize {
        let mut seen = 0;
        for (position, row) in self.rows().iter().enumerate() {
            if matches!(row, Row::Task(_)) {
                if seen == self.ui.cursor {
                    return position;
                }
                seen += 1;
            }
        }
        0
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
            let indices = self.bucket_view(Self::BUCKETS[target as usize]).0;
            if !indices.is_empty() {
                let base: usize = Self::BUCKETS[..target as usize]
                    .iter()
                    .map(|bucket| self.bucket_view(*bucket).0.len())
                    .sum();
                self.ui.cursor = base + local.min(indices.len() - 1);
                return;
            }
            target += delta;
        }
    }

    /// The cursor's `(bucket position, row within bucket)`, if any.
    fn cursor_bucket_position(&self) -> Option<(usize, usize)> {
        let mut seen = 0;
        for (position, bucket) in Self::BUCKETS.iter().enumerate() {
            for local in 0..self.bucket_view(*bucket).0.len() {
                if seen == self.ui.cursor {
                    return Some((position, local));
                }
                seen += 1;
            }
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

    /// Completed tasks matching the current search filter (case-insensitive).
    pub fn done_filtered(&self) -> Vec<usize> {
        let needle = self.ui.done_filter.value().to_lowercase();
        self.board
            .done()
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

    pub fn set_toast(&mut self, text: impl Into<String>) {
        self.ui.toast = Some(Toast {
            text: text.into(),
            born: self.now,
        });
    }
}
