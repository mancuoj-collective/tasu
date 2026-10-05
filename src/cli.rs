use clap::{Parser, Subcommand, ValueEnum};

use crate::domain::Bucket;

#[derive(Debug, Parser)]
#[command(name = "tasu", version, about = "A terminal todo list that ages.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// A bucket name, for `list` and `move`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BucketName {
    Today,
    Week,
    Later,
}

impl From<BucketName> for Bucket {
    fn from(value: BucketName) -> Self {
        match value {
            BucketName::Today => Bucket::Today,
            BucketName::Week => Bucket::Week,
            BucketName::Later => Bucket::Later,
        }
    }
}

/// Which list the history command shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HistoryName {
    Done,
    Dropped,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a task to today without opening the TUI.
    #[command(visible_alias = "a")]
    Add {
        /// The task title.
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// Print the open tasks, grouped by bucket.
    #[command(visible_alias = "ls")]
    List {
        /// Only show this bucket.
        #[arg(value_enum)]
        bucket: Option<BucketName>,
        /// Output a JSON array.
        #[arg(long)]
        json: bool,
    },
    /// Print completed and dropped tasks.
    #[command(visible_alias = "h")]
    History {
        /// Which view to show; omit to show both.
        #[arg(value_enum)]
        view: Option<HistoryName>,
        /// Output a JSON array.
        #[arg(long)]
        json: bool,
    },
    /// Complete an open task, matched by its exact title.
    #[command(visible_alias = "d")]
    Done {
        /// The exact title of an open task.
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// Drop (archive) an open task, matched by its exact title.
    #[command(visible_alias = "x")]
    Drop {
        /// The exact title of an open task.
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// Move an open task to a bucket, matched by its exact title.
    #[command(visible_alias = "mv")]
    Move {
        /// The target bucket.
        #[arg(value_enum)]
        bucket: BucketName,
        /// The exact title of an open task.
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// Show the resolved data directory, config file and sync remote.
    Config {
        /// Output a JSON object.
        #[arg(long)]
        json: bool,
    },
    /// Show, set or clear the git sync remote.
    Remote {
        /// Remote URL, local path, or a GitHub `owner/repo` shorthand.
        url: Option<String>,
        /// Disable syncing.
        #[arg(long)]
        clear: bool,
    },
    /// Run one pull-then-push now and report the result.
    Sync,
    /// Update tasu, using however it was installed (Homebrew, the installer
    /// script, or Cargo).
    Update,
    /// Print a shell completion script.
    Completions {
        /// The shell to generate for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}
