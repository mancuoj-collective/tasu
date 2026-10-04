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
    /// Show the resolved data directory, config file and sync remote.
    Config,
    /// Show, set or clear the git sync remote.
    Remote {
        /// Remote URL to sync with, e.g. git@github.com:you/tasu-data.git.
        url: Option<String>,
        /// Disable syncing.
        #[arg(long)]
        clear: bool,
    },
}
