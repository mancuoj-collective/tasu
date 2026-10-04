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
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            cursor: 0,
            input: Input::default(),
            toast: None,
            done_cursor: 0,
        }
    }
}

/// A rendered row: a bucket header or a task, in display order.
#[derive(Debug, Clone, Copy)]
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

    /// Display rows: each bucket header followed by its tasks (newest first).
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for bucket in Self::BUCKETS {
            rows.push(Row::Header(bucket));
            for index in self.board.open_in(bucket) {
                rows.push(Row::Task(index));
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

    pub fn clamp_cursor(&mut self) {
        let len = self.selectable_len();
        if len == 0 {
            self.ui.cursor = 0;
        } else if self.ui.cursor >= len {
            self.ui.cursor = len - 1;
        }
    }

    pub fn set_toast(&mut self, text: impl Into<String>) {
        self.ui.toast = Some(Toast {
            text: text.into(),
            born: self.now,
        });
    }
}
