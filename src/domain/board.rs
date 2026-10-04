use chrono::{DateTime, Local};

use super::task::{Bucket, Task, TaskState};

/// The whole task collection. Every operation takes `now` so the domain stays
/// free of the system clock and is deterministic under test.
#[derive(Debug, Default, Clone)]
pub struct Board {
    tasks: Vec<Task>,
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_tasks(tasks: Vec<Task>) -> Self {
        Self { tasks }
    }

    pub fn into_tasks(self) -> Vec<Task> {
        self.tasks
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub(crate) fn tasks_mut(&mut self) -> &mut [Task] {
        &mut self.tasks
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn task(&self, index: usize) -> Option<&Task> {
        self.tasks.get(index)
    }

    /// Add a task to `Today`. Returns its index.
    pub fn add(&mut self, title: impl Into<String>, now: DateTime<Local>) -> usize {
        self.tasks.push(Task::new(title, now));
        self.tasks.len() - 1
    }

    pub fn rename(&mut self, index: usize, title: impl Into<String>) -> bool {
        let title = title.into();
        if title.trim().is_empty() {
            return false;
        }
        match self.tasks.get_mut(index) {
            Some(task) => {
                task.title = title;
                true
            }
            None => false,
        }
    }

    /// Complete an open task. Keeps bucket info so [`Board::restore`] can undo.
    pub fn complete(&mut self, index: usize, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Open {
            return false;
        }
        task.state = TaskState::Done;
        task.completed_at = Some(now);
        true
    }

    /// Undo a completion: reopen in the original bucket, at the original position.
    pub fn restore(&mut self, index: usize, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Done {
            return false;
        }
        task.state = TaskState::Open;
        task.completed_at = None;
        task.bucket_since = now;
        true
    }

    pub fn archive(&mut self, index: usize, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Open {
            return false;
        }
        task.state = TaskState::Archived;
        task.archived_at = Some(now);
        true
    }

    pub fn unarchive(&mut self, index: usize, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Archived {
            return false;
        }
        task.state = TaskState::Open;
        task.archived_at = None;
        task.bucket_since = now;
        true
    }

    /// Shift an open task `delta` steps along the pipeline.
    pub fn move_bucket(&mut self, index: usize, delta: i32, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Open {
            return false;
        }
        let next = task.bucket.shift(delta);
        if next == task.bucket {
            return false;
        }
        task.bucket = next;
        task.bucket_since = now;
        true
    }

    /// Pull an open task directly into `Today`.
    pub fn pin_today(&mut self, index: usize, now: DateTime<Local>) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };
        if task.state != TaskState::Open || task.bucket == Bucket::Today {
            return false;
        }
        task.bucket = Bucket::Today;
        task.bucket_since = now;
        true
    }

    /// Open tasks in a bucket, newest first.
    pub fn open_in(&self, bucket: Bucket) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.state == TaskState::Open && task.bucket == bucket)
            .map(|(index, _)| index)
            .collect();
        indices.sort_by(|&a, &b| self.tasks[b].created_at.cmp(&self.tasks[a].created_at));
        indices
    }

    /// Completed tasks, most recently completed first.
    pub fn done(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.state == TaskState::Done)
            .map(|(index, _)| index)
            .collect();
        indices.sort_by(|&a, &b| self.tasks[b].completed_at.cmp(&self.tasks[a].completed_at));
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    fn seeded(titles: &[&str]) -> Board {
        let mut board = Board::new();
        for (i, title) in titles.iter().enumerate() {
            board.add(*title, at(2026, 10, 5 + i as u32));
        }
        board
    }

    #[test]
    fn new_tasks_start_open_in_today() {
        let mut board = Board::new();
        board.add("first", at(2026, 10, 5));
        let task = board.task(0).unwrap();
        assert_eq!(task.bucket, Bucket::Today);
        assert_eq!(task.state, TaskState::Open);
    }

    #[test]
    fn rename_rejects_blank_titles() {
        let mut board = seeded(&["keep me"]);
        assert!(!board.rename(0, "   "));
        assert_eq!(board.task(0).unwrap().title, "keep me");
        assert!(board.rename(0, "renamed"));
        assert_eq!(board.task(0).unwrap().title, "renamed");
    }

    #[test]
    fn restore_returns_task_to_its_original_bucket() {
        let mut board = seeded(&["promote me"]);
        board.move_bucket(0, 1, at(2026, 10, 6));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Week);

        board.complete(0, at(2026, 10, 7));
        assert!(board.restore(0, at(2026, 10, 8)));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Week);
        assert_eq!(board.task(0).unwrap().state, TaskState::Open);
    }

    #[test]
    fn unarchive_returns_task_to_its_original_bucket() {
        let mut board = seeded(&["reconsider"]);
        board.move_bucket(0, 2, at(2026, 10, 6));
        board.archive(0, at(2026, 10, 7));

        assert!(board.unarchive(0, at(2026, 10, 8)));
        assert_eq!(board.task(0).unwrap().bucket, Bucket::Later);
    }

    #[test]
    fn move_bucket_clamps_at_both_ends() {
        let mut board = seeded(&["edge"]);
        assert!(!board.move_bucket(0, -1, at(2026, 10, 6))); // Today promoted = Today
        assert!(board.move_bucket(0, 2, at(2026, 10, 6))); // drops to Later
        assert!(!board.move_bucket(0, 1, at(2026, 10, 6))); // Later demoted = Later
    }

    #[test]
    fn open_in_lists_newest_first() {
        let board = seeded(&["oldest", "middle", "newest"]);
        assert_eq!(board.open_in(Bucket::Today), vec![2, 1, 0]);
    }

    #[test]
    fn done_lists_most_recent_completion_first() {
        let mut board = seeded(&["a", "b"]);
        board.complete(0, at(2026, 10, 8));
        board.complete(1, at(2026, 10, 9));
        assert_eq!(board.done(), vec![1, 0]);
    }
}
