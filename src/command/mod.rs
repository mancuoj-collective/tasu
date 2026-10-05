use std::io::IsTerminal;

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
    if let Err(err) = sync::pull_now(&config.data_dir, config.remote.as_deref()) {
        eprintln!("tasu: pull failed: {err}");
    }

    let now = Local::now();
    let mut board = store.load();
    settle(&mut board, now);
    board.add(title, now);
    store.save(&board)?;

    if let Err(err) = sync::commit_now(&config.data_dir, config.remote.as_deref()) {
        eprintln!("tasu: push failed: {err}");
    }
    Ok(())
}

/// `tasu sync`: force one pull-then-push and report what happened.
pub fn sync(config: &Config) -> Result<()> {
    let Some(url) = config.remote.as_deref() else {
        println!("sync off (local only); set one with: tasu remote <owner/repo>");
        return Ok(());
    };
    println!("syncing {url}");

    if let Err(err) = crate::sync::pull_now(&config.data_dir, Some(url)) {
        println!("pull failed:\n  {err}");
    }
    match crate::sync::commit_now(&config.data_dir, Some(url)) {
        Ok(()) => println!("push ok"),
        Err(err) => println!("push failed:\n  {err}"),
    }
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
        // Verify the remote before committing it to the config. Persisting an
        // unreachable or unauthorized URL would leave every later run retrying
        // a sync that can never succeed.
        if let Err(err) = sync::probe_remote(&url) {
            Config::set_remote(None)?;
            anyhow::bail!("{}", unreachable(&url, &err));
        }
        let path = Config::set_remote(Some(&url))?;
        println!("sync remote set to {url}");
        println!("config written to {}", path.display());
        println!("remote reachable, credentials OK");
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

/// A short, prioritised explanation of a failed probe: lead with the cause,
/// show the URL once, then one fix. The raw git error only appears when it
/// cannot be classified.
fn unreachable(url: &str, error: &str) -> String {
    let kind = sync::Failure::classify(error);
    let mut message = format!(
        "{} {}\n",
        styled("1;31", "\u{2717}"),
        styled("1;31", &format!("remote not set \u{b7} {}", kind.reason())),
    );
    message.push_str(&format!("  {url}\n"));
    if kind == sync::Failure::Other
        && let Some(line) = error.lines().map(str::trim).find(|line| !line.is_empty())
    {
        message.push_str(&format!("  {}\n", styled("2", line)));
    }
    message.push_str(&format!("  {} {}\n", styled("36", "\u{2192}"), kind.fix()));
    message.push_str(&format!(
        "  {}",
        styled("2", "sync stays off (previous remote cleared)")
    ));
    message
}

/// ANSI styling, only when stderr is a terminal and `NO_COLOR` is unset.
fn styled(code: &str, text: &str) -> String {
    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    if color {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
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

    #[test]
    fn unreachable_message_leads_with_the_cause_and_shows_the_url_once() {
        let url = "https://github.com/mancuoj/tasu-data.git";
        let message = super::unreachable(url, "remote: Repository not found.");
        assert!(
            message.starts_with("\u{2717} remote not set \u{b7} repository not found"),
            "{message}"
        );
        assert_eq!(message.matches(url).count(), 1, "{message}");
    }
}
