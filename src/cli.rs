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
}
