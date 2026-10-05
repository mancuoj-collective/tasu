use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tasu", version, about = "A terminal todo list that ages.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a task to Today without opening the TUI.
    Add {
        /// The task title.
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// Print the open tasks, grouped by bucket.
    #[command(alias = "ls")]
    List,
    /// Show the resolved data directory, config file and sync remote.
    Config,
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
}
