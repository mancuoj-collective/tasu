use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Where a task currently sits in the aging pipeline.
///
/// `Today` and `Week` are time-scoped and demote automatically; `Later` is the
/// terminal bucket. See [`crate::domain::settle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Today,
    Week,
    Later,
}

impl Bucket {
    /// Position in the pipeline, used to shift between buckets.
    pub fn index(self) -> usize {
        match self {
            Bucket::Today => 0,
            Bucket::Week => 1,
            Bucket::Later => 2,
        }
    }

    /// Move `delta` steps along the pipeline, clamped at both ends.
    pub fn shift(self, delta: i32) -> Bucket {
        match self.index() as i32 + delta {
            ..=0 => Bucket::Today,
            1 => Bucket::Week,
            _ => Bucket::Later,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Open,
    Done,
    Archived,
}

/// A single task. Carries no id: identity is positional and bucket order is
/// derived from `created_at`, so restoring a task needs no extra bookkeeping.
///
/// `bucket` is preserved across `Done`/`Archived` so undo can return a task to
/// the bucket it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    pub state: TaskState,
    pub bucket: Bucket,
    /// When the task entered its current bucket by user intent. Automatic
    /// demotion leaves it untouched so a stale task can cascade further.
    pub bucket_since: DateTime<Local>,
    pub created_at: DateTime<Local>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Local>>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Local>>,
}

impl Task {
    pub fn new(title: impl Into<String>, now: DateTime<Local>) -> Self {
        Self {
            title: title.into(),
            state: TaskState::Open,
            bucket: Bucket::Today,
            bucket_since: now,
            created_at: now,
            completed_at: None,
            archived_at: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == TaskState::Open
    }
}
