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

    /// Merge another board in, task by task. Tasks carry no id, so identity is
    /// `created_at` — unique at nanosecond precision and, unlike the title,
    /// stable across renames. For a matching task the one that is further along
    /// wins — a terminal state (`done` / `archived`) over `open`, then the later
    /// change time — so merging is idempotent and two machines that edited the
    /// same task do not drift into duplicates. A task only on the other side
    /// is added; `self` wins true ties.
    pub fn merged_with(mut self, other: &Board) -> Board {
        self.dedup();
        for task in &other.tasks {
            match self
                .tasks
                .iter()
                .position(|existing| same_task(existing, task))
            {
                Some(index) => {
                    if is_further(task, &self.tasks[index]) {
                        self.tasks[index] = task.clone();
                    }
                }
                None => self.tasks.push(task.clone()),
            }
        }
        self
    }

    /// Collapse tasks that share a creation time, keeping the one that is
    /// further along. A board should never hold two — creation time is the
    /// identity — but older syncs could leave duplicates behind.
    pub fn dedup(&mut self) {
        let mut unique: Vec<Task> = Vec::new();
        for task in self.tasks.drain(..) {
            match unique
                .iter_mut()
                .find(|existing| same_task(existing, &task))
            {
                Some(existing) => {
                    if is_further(&task, existing) {
                        *existing = task;
                    }
                }
                None => unique.push(task),
            }
        }
        self.tasks = unique;
    }

    /// Add a task to `Today`. Returns its index.
    ///
    /// `created_at` is the task identity, so two tasks must never share an
    /// instant: the caller's `now` is nudged forward by a nanosecond until it
    /// is unique. Otherwise a bulk add (or two adds on a coarse clock) would
    /// silently collapse into one task on the next merge.
    pub fn add(&mut self, title: impl Into<String>, now: DateTime<Local>) -> usize {
        let created = self.unique_instant(now);
        self.tasks.push(Task::new(title, created));
        self.tasks.len() - 1
    }

    fn unique_instant(&self, mut now: DateTime<Local>) -> DateTime<Local> {
        let step = chrono::TimeDelta::nanoseconds(1);
        while self.tasks.iter().any(|task| task.created_at == now) {
            now += step;
        }
        now
    }

    pub fn rename(&mut self, index: usize, title: impl Into<String>, now: DateTime<Local>) -> bool {
        let title = title.into();
        if title.trim().is_empty() {
            return false;
        }
        match self.tasks.get_mut(index) {
            Some(task) => {
                task.title = title;
                task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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
        task.updated_at = Some(now);
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

    /// Archived (dropped) tasks, most recently archived first.
    pub fn archived(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.state == TaskState::Archived)
            .map(|(index, _)| index)
            .collect();
        indices.sort_by(|&a, &b| self.tasks[b].archived_at.cmp(&self.tasks[a].archived_at));
        indices
    }
}

/// Task identity without an id: the instant it was created. `created_at` is
/// preserved when a task is copied between machines and when it is renamed, so
/// it names the same task on both sides.
fn same_task(a: &Task, b: &Task) -> bool {
    a.created_at == b.created_at
}

/// Whether `candidate` is further along than `current`: a terminal state beats
/// `open`, and within a state the more recently changed copy wins. The change
/// time is what lets a rename — which moves no other field — win a tie.
fn is_further(candidate: &Task, current: &Task) -> bool {
    state_rank(candidate.state) > state_rank(current.state)
        || (candidate.state == current.state && candidate.updated() > current.updated())
}

fn state_rank(state: TaskState) -> u8 {
    match state {
        TaskState::Open => 0,
        TaskState::Done => 1,
        TaskState::Archived => 2,
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

    /// A board with explicit `(title, day)` so a test can line up or separate
    /// creation times, which are the task identity.
    fn seeded_at(pairs: &[(&str, u32)]) -> Board {
        let mut board = Board::new();
        for (title, day) in pairs {
            board.add(*title, at(2026, 10, *day));
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
        assert!(!board.rename(0, "   ", at(2026, 10, 6)));
        assert_eq!(board.task(0).unwrap().title, "keep me");
        assert!(board.rename(0, "renamed", at(2026, 10, 6)));
        assert_eq!(board.task(0).unwrap().title, "renamed");
    }

    #[test]
    fn a_later_rename_wins_the_merge() {
        // A rename moves no other field, so without a change time it could not
        // beat the other side's copy. The later rename must survive whichever
        // side is the base.
        let base = Board::from_tasks(vec![Task::new("old name", at(2026, 10, 5))]);
        let mut renamed = Board::from_tasks(vec![Task::new("old name", at(2026, 10, 5))]);
        renamed.rename(0, "new name", at(2026, 10, 6));

        for merged in [
            base.clone().merged_with(&renamed),
            renamed.clone().merged_with(&base),
        ] {
            assert_eq!(merged.task(0).unwrap().title, "new name");
        }
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
    fn merging_matches_tasks_by_creation_time() {
        let left = seeded_at(&[("shared", 5), ("only left", 6)]);
        let right = seeded_at(&[("shared", 5), ("only right", 7)]);
        let merged = left.merged_with(&right);
        let titles: Vec<&str> = merged
            .tasks()
            .iter()
            .map(|task| task.title.as_str())
            .collect();
        assert_eq!(titles, vec!["shared", "only left", "only right"]);
    }

    #[test]
    fn a_rename_does_not_fork_the_task() {
        // Same instant, different title: the same task, renamed.
        let before = seeded_at(&[("cat food", 5)]);
        let after = seeded_at(&[("buy cat food", 5)]);
        let merged = before.merged_with(&after);
        assert_eq!(merged.len(), 1, "a rename must not become a second task");
    }

    #[test]
    fn dedup_collapses_same_instant_copies() {
        // Two copies of one task at the same instant (an older sync could leave
        // this) collapse to the further-along one. Built directly, because
        // `add` now guarantees distinct instants.
        let mut board = Board::from_tasks(vec![
            Task::new("dup", at(2026, 10, 5)),
            Task::new("dup", at(2026, 10, 5)),
        ]);
        board.complete(1, at(2026, 10, 6));
        board.dedup();
        assert_eq!(board.len(), 1);
        assert_eq!(board.task(0).unwrap().state, TaskState::Done);
    }

    #[test]
    fn add_never_reuses_an_instant() {
        let mut board = Board::new();
        board.add("one", at(2026, 10, 5));
        board.add("two", at(2026, 10, 5));
        assert_ne!(
            board.task(0).unwrap().created_at,
            board.task(1).unwrap().created_at
        );
        board.dedup();
        assert_eq!(board.len(), 2, "distinct instants must not collapse");
    }

    #[test]
    fn merging_prefers_the_task_that_is_further_along() {
        let mut open = Board::new();
        open.add("ship it", at(2026, 10, 5));
        let mut done = Board::new();
        done.add("ship it", at(2026, 10, 5));
        done.complete(0, at(2026, 10, 6));

        // Either merge direction yields one task, completed.
        for merged in [
            open.clone().merged_with(&done),
            done.clone().merged_with(&open),
        ] {
            assert_eq!(merged.len(), 1, "the two copies must collapse");
            assert_eq!(merged.task(0).unwrap().state, TaskState::Done);
        }
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
