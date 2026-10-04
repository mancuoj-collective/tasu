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

/// `tasu remote`: show, set or clear the sync remote. Accepts a full URL, a
/// local path, or a GitHub `owner/repo` shorthand.
pub fn remote(config: &Config, spec: Option<&str>, clear: bool) -> Result<()> {
    if clear {
        Config::set_remote(None)?;
        println!("sync disabled");
    } else if let Some(spec) = spec {
        let url = normalize_remote(spec);
        let path = Config::set_remote(Some(&url))?;
        println!("sync remote set to {url}");
        println!("config written to {}", path.display());

        match sync::probe_remote(&url) {
            Ok(()) => println!("remote reachable, credentials OK"),
            Err(err) => {
                println!("could not reach the remote:");
                println!("  {err}");
                println!(
                    "hint: over HTTPS git needs a stored token. On macOS a keychain \
                     helper usually has one from previous clones; otherwise create a \
                     personal access token and let the helper store it, or use an SSH \
                     remote instead."
                );
            }
        }
    } else {
        println!("{}", config.remote.as_deref().unwrap_or("none"));
    }
    Ok(())
}

/// Expand a GitHub `owner/repo` shorthand to an HTTPS URL. Anything that already
/// looks like a URL, an scp-style address, or a local path is left alone.
pub fn normalize_remote(spec: &str) -> String {
    let spec = spec.trim();
    let looks_like_url = spec.contains("://")
        || spec.contains(':')
        || spec.starts_with("git@")
        || spec.starts_with('/')
        || spec.starts_with('~')
        || spec.starts_with('.');
    if looks_like_url {
        return spec.to_string();
    }

    if let Some((host, _)) = spec.split_once('/') {
        // A bare `host.tld/path` gets an https scheme; `owner/repo` is GitHub.
        if host.contains('.') {
            return format!("https://{spec}");
        }
        let mut url = format!("https://github.com/{spec}");
        if !url.ends_with(".git") {
            url.push_str(".git");
        }
        return url;
    }

    spec.to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_remote;

    #[test]
    fn expands_github_shorthand() {
        assert_eq!(
            normalize_remote("mancuoj/tasu-data"),
            "https://github.com/mancuoj/tasu-data.git"
        );
        assert_eq!(
            normalize_remote("mancuoj/tasu-data.git"),
            "https://github.com/mancuoj/tasu-data.git"
        );
    }

    #[test]
    fn leaves_urls_and_paths_alone() {
        for spec in [
            "https://github.com/a/b.git",
            "git@github.com:a/b.git",
            "/tmp/remote.git",
            "~/Sync/tasu-remote.git",
            "./relative.git",
            "ssh://host/path",
        ] {
            assert_eq!(normalize_remote(spec), spec);
        }
    }

    #[test]
    fn adds_a_scheme_to_a_bare_host() {
        assert_eq!(normalize_remote("github.com/a/b"), "https://github.com/a/b");
    }
}
