//! One-shot commands: each loads the board, does one thing and exits. The task
//! commands share one shape, [`change_board`]; the rest live in their own files.

mod remote;
mod render;
mod update;

use anyhow::Result;
use chrono::{DateTime, Local};

use crate::config::Config;
use crate::domain::{Board, Bucket, Task, TaskState, settle};
use crate::store::Store;
use crate::sync;

pub use remote::remote;
pub use update::update;

use render::{bucket_label, history_sections, render_history, render_open};

/// Load the board for a CLI command. A file that cannot be read has been moved
/// aside by the store; the command still runs on an empty board, but the user is
/// told on stderr so the loss is never silent.
fn load_board(store: &Store) -> Board {
    let loaded = store.load();
    if let Some(damage) = &loaded.damaged {
        eprintln!("tasu: {}", damage.detail());
    }
    loaded.board
}

/// Open the local board for a one-shot command: the `Store` to write it back
/// with, and the board settled to `now`. Every command that mutates the board
/// settles it first, so the aging pipeline is applied before the change.
fn open_board(config: &Config, now: DateTime<Local>) -> (Store, Board) {
    let store = Store::new(config.board_path());
    let mut board = load_board(&store);
    settle(&mut board, now);
    (store, board)
}

/// The shared shape of every command that changes the board: pull, apply
/// `change` to the settled board, save, report, then push. The pull and push are
/// best-effort so an offline change is still captured; failures are reported on
/// stderr. A change that is refused is never saved.
fn change_board(
    config: &Config,
    change: impl FnOnce(&mut Board, DateTime<Local>) -> Result<String>,
) -> Result<()> {
    best_effort_pull(config);
    let now = Local::now();
    let (store, mut board) = open_board(config, now);
    let message = change(&mut board, now)?;
    store.save(&board)?;
    println!("{message}");
    best_effort_push(config);
    Ok(())
}

/// `tasu add "..."`: record a task and push, without opening the TUI. The push
/// is best-effort: offline still exits successfully.
pub fn add(config: &Config, words: &[String]) -> Result<()> {
    let title = required_title(words)?;
    change_board(config, |board, now| {
        board.add(title.as_str(), now);
        Ok(format!("+ {title}"))
    })
}

/// `tasu list`: print the open tasks, grouped by bucket. Read-only and local —
/// run `tasu sync` first if you want the latest from the remote.
pub fn list(config: &Config, bucket: Option<Bucket>, json: bool) -> Result<()> {
    let (_, board) = open_board(config, Local::now());

    let buckets = match bucket {
        Some(bucket) => vec![bucket],
        None => vec![Bucket::Today, Bucket::Week, Bucket::Later],
    };
    let tasks: Vec<&Task> = buckets
        .iter()
        .flat_map(|bucket| board.open_in(*bucket))
        .filter_map(|index| board.task(index))
        .collect();

    if json {
        return print_json(&tasks);
    }
    print!("{}", render_open(&board, &buckets));
    Ok(())
}

/// Which history lists `tasu history` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryList {
    Both,
    Done,
    Dropped,
}

/// `tasu history [done|dropped]`: print completed and dropped tasks. With no
/// view, both sections are shown.
pub fn history(config: &Config, view: HistoryList, json: bool) -> Result<()> {
    let board = load_board(&Store::new(config.board_path()));
    let sections = history_sections(&board, view);

    if json {
        let tasks: Vec<&Task> = sections
            .iter()
            .flat_map(|(_, tasks)| tasks.iter().copied())
            .collect();
        return print_json(&tasks);
    }
    print!("{}", render_history(&sections, view));
    Ok(())
}

/// `tasu done "<title>"`: complete an open task by its exact title.
pub fn done(config: &Config, words: &[String]) -> Result<()> {
    finish(config, words, false)
}

/// `tasu drop "<title>"`: drop (archive) an open task by its exact title.
pub fn drop_task(config: &Config, words: &[String]) -> Result<()> {
    finish(config, words, true)
}

/// Shared body of `done` and `drop`: resolve the title, then act on it.
fn finish(config: &Config, words: &[String], drop: bool) -> Result<()> {
    let title = required_title(words)?;
    change_board(config, |board, now| {
        let index = find_open(board, &title)?;
        let changed = if drop {
            board.archive(index, now)
        } else {
            board.complete(index, now)
        };
        if !changed {
            anyhow::bail!("could not update {title:?}");
        }
        Ok(if drop {
            format!("\u{2717} dropped {title:?}")
        } else {
            format!("\u{2713} completed {title:?}")
        })
    })
}

/// `tasu move <bucket> "<title>"`: send an open task to a bucket by its exact
/// title.
pub fn move_task(config: &Config, bucket: Bucket, words: &[String]) -> Result<()> {
    let title = required_title(words)?;
    change_board(config, |board, now| {
        let index = find_open(board, &title)?;
        board.set_bucket(index, bucket, now);
        Ok(format!(
            "\u{2192} moved {title:?} to {}",
            bucket_label(bucket).to_lowercase()
        ))
    })
}

/// Resolve a title to a single open task. Errors on none or several matches:
/// tasks carry no id, so the title has to be exact and unambiguous.
fn find_open(board: &Board, title: &str) -> Result<usize> {
    let matches: Vec<usize> = board
        .tasks()
        .iter()
        .enumerate()
        .filter(|(_, task)| task.state == TaskState::Open && task.title == title)
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [] => anyhow::bail!("no open task titled {title:?}"),
        [index] => Ok(*index),
        many => anyhow::bail!(
            "{} open tasks titled {title:?}; rename one to disambiguate",
            many.len()
        ),
    }
}

fn required_title(words: &[String]) -> Result<String> {
    joined_title(words).ok_or_else(|| anyhow::anyhow!("a title is required"))
}

fn joined_title(words: &[String]) -> Option<String> {
    let joined = words.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Best-effort pull before a one-shot command. The command still runs on the
/// local board, but a failed pull is reported like a failed push, never silent.
/// A local board that could not be read would otherwise be replaced by the
/// remote's copy, so its damage is printed too.
fn best_effort_pull(config: &Config) {
    match sync::pull_now(&config.data_dir, config.remote.as_deref()) {
        Ok(Some(damage)) => eprintln!("tasu: {}", damage.detail()),
        Ok(None) => {}
        Err(err) => eprintln!("tasu: pull failed: {err}"),
    }
}

fn best_effort_push(config: &Config) {
    if let Err(err) = sync::commit_now(&config.data_dir, config.remote.as_deref()) {
        eprintln!("tasu: push failed: {err}");
    }
}

fn print_json(tasks: &[&Task]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(tasks)?);
    Ok(())
}

/// `tasu sync`: force one pull-then-push and report what happened. A failure
/// exits non-zero, so scripts can tell.
pub fn sync(config: &Config) -> Result<()> {
    let Some(url) = config.remote.as_deref() else {
        println!("sync off (local only); set one with: tasu remote <owner/repo>");
        return Ok(());
    };
    println!("syncing {url}");

    let pull = sync::pull_now(&config.data_dir, Some(url));
    let push = sync::commit_now(&config.data_dir, Some(url));
    if let Ok(Some(damage)) = &pull {
        eprintln!("tasu: {}", damage.detail());
    }
    if let Err(err) = &pull {
        eprintln!("pull failed:\n  {err}");
    }
    match &push {
        Ok(()) => println!("push ok"),
        Err(err) => eprintln!("push failed:\n  {err}"),
    }
    if pull.is_err() || push.is_err() {
        anyhow::bail!("sync failed");
    }
    Ok(())
}

/// Internal: commit and push whatever is pending, then exit. The TUI spawns
/// this detached on quit, so quitting never waits on the network.
pub fn flush(config: &Config) -> Result<()> {
    best_effort_push(config);
    Ok(())
}

/// `tasu config`: show the resolved settings.
pub fn config(config: &Config, json: bool) -> Result<()> {
    if json {
        let value = serde_json::json!({
            "data_dir": config.data_dir.display().to_string(),
            "board": config.board_path().display().to_string(),
            "config": Config::config_file().map(|path| path.display().to_string()),
            "remote": config.remote.as_deref(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("data dir   {}", config.data_dir.display());
    println!("board      {}", config.board_path().display());
    if let Some(path) = Config::config_file() {
        println!("config     {}", path.display());
    }
    match &config.remote {
        Some(url) => println!("sync       {url}"),
        None => println!("sync       off (local only)"),
    }
    Ok(())
}

/// `tasu completions <shell>`: print a completion script to stdout.
pub fn completions(shell: clap_complete::Shell) -> Result<()> {
    use clap::CommandFactory;
    let mut command = crate::cli::Cli::command();
    clap_complete::generate(shell, &mut command, "tasu", &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    #[test]
    fn find_open_requires_a_unique_exact_title() {
        let mut board = Board::new();
        board.add("write tests", at(2026, 10, 5));
        board.add("write tests", at(2026, 10, 5));
        board.add("other", at(2026, 10, 5));
        board.add("solo", at(2026, 10, 5));
        board.complete(2, at(2026, 10, 5));

        assert!(find_open(&board, "solo").is_ok());
        assert!(find_open(&board, "missing").is_err());
        assert!(
            find_open(&board, "other").is_err(),
            "done tasks must not match"
        );
        let ambiguous = find_open(&board, "write tests").unwrap_err().to_string();
        assert!(ambiguous.contains("disambiguate"), "{ambiguous}");
    }

    #[test]
    fn done_completes_a_unique_title() {
        use crate::store::Store;

        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            remote: None,
        };
        add(&config, &["buy milk".to_string()]).unwrap();
        done(&config, &["buy milk".to_string()]).unwrap();

        let board = Store::new(config.board_path()).load().board;
        assert_eq!(board.task(0).unwrap().state, TaskState::Done);
    }
}
