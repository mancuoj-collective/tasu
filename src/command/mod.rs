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

    // Best-effort pull before reading, so a machine that only ever uses the
    // CLI still converges with the remote.
    let _ = sync::pull_now(&config.data_dir, config.remote.as_deref());

    let now = Local::now();
    let mut board = store.load();
    settle(&mut board, now);
    board.add(title, now);
    store.save(&board)?;

    let _ = sync::commit_now(&config.data_dir, config.remote.as_deref());
    Ok(())
}

/// `tasu config`: show the resolved settings.
pub fn config(config: &Config) {
    println!("data dir   {}", config.data_dir.display());
    println!("board      {}", config.board_path().display());
    if let Some(path) = Config::config_file() {
        println!("config     {}", path.display());
    }
    match &config.remote {
        Some(url) => println!("sync       {url}"),
        None => println!("sync       off (local only)"),
    }
}

/// `tasu remote`: show, set or clear the sync remote.
pub fn remote(config: &Config, url: Option<&str>, clear: bool) -> Result<()> {
    if clear {
        Config::set_remote(None)?;
        println!("sync disabled");
    } else if let Some(url) = url {
        let path = Config::set_remote(Some(url))?;
        println!("sync remote set to {url}");
        println!("config written to {}", path.display());
    } else {
        println!("{}", config.remote.as_deref().unwrap_or("none"));
    }
    Ok(())
}
