use anyhow::Result;
use chrono::Local;

use crate::config::Config;
use crate::domain::settle;
use crate::store::Store;
use crate::sync;

/// `tasu add "..."`: record a task and push, without opening the TUI. The push
/// is best-effort: offline still exits successfully.
pub fn add(config: &Config, words: &[String]) -> Result<()> {
    let title = words.join(" ");
    let title = title.trim();
    if title.is_empty() {
        return Ok(());
    }

    let store = Store::new(config.board_path());
    let now = Local::now();
    let mut board = store.load();
    settle(&mut board, now);
    board.add(title, now);
    store.save(&board)?;

    let _ = sync::commit_now(&config.data_dir, config.remote.as_deref());
    Ok(())
}
