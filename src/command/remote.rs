//! `tasu remote`: show, set or clear the sync remote, and explain why a remote
//! was refused.

use std::io::IsTerminal;

use anyhow::Result;

use crate::config::Config;
use crate::sync::{self, Failure};

/// `tasu remote`: show, set or clear the sync remote. Accepts a full URL, a
/// local path, or a GitHub `owner/repo` shorthand.
pub fn remote(config: &Config, spec: Option<&str>, clear: bool) -> Result<()> {
    let shadow = env_override();
    if clear {
        Config::set_remote(None)?;
        println!("sync disabled");
        if let Some(url) = &shadow {
            eprintln!("{}", env_shadow_warning(url));
        }
    } else if let Some(spec) = spec {
        let url = normalize_remote(spec);
        // Verify the remote before committing it to the config. Persisting an
        // unreachable or unauthorized URL would leave every later run retrying
        // a sync that can never succeed.
        if let Err(err) = sync::probe_remote(&url) {
            // A definitively bad remote (missing repository, rejected
            // credentials) is cleared. A transient failure (offline) leaves the
            // current remote untouched, so a network blip cannot wipe a working
            // setting.
            let cleared = matches!(err.kind(), Failure::NotFound | Failure::Auth);
            if cleared {
                Config::set_remote(None)?;
            }
            anyhow::bail!("{}", unreachable(&url, &err, cleared));
        }
        let path = Config::set_remote(Some(&url))?;
        println!("sync remote set to {url}");
        println!("config written to {}", path.display());
        println!("remote reachable, credentials OK");
        if let Some(env_url) = &shadow {
            eprintln!("{}", env_shadow_warning(env_url));
        }
    } else {
        println!("{}", config.remote.as_deref().unwrap_or("none"));
    }
    Ok(())
}

/// The `TASU_REMOTE` value, when set to something non-empty. It takes precedence
/// over the config file, so a change to the file is shadowed while it is set.
fn env_override() -> Option<String> {
    std::env::var_os("TASU_REMOTE")
        .filter(|value| !value.is_empty())
        .and_then(|value| value.into_string().ok())
}

/// Warn that `TASU_REMOTE` shadows the config file just written, so a "sync
/// disabled" or "remote set" line is not read as the effective state.
fn env_shadow_warning(env_remote: &str) -> String {
    format!(
        "tasu: TASU_REMOTE is set ({env_remote}) and overrides the config file; \
         this change has no effect until it is unset"
    )
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
/// cannot be classified. `cleared` says whether the previous remote was dropped
/// or kept, so the message matches what actually happened.
fn unreachable(url: &str, error: &sync::Error, cleared: bool) -> String {
    let kind = error.kind();
    let headline = if cleared {
        "remote not set"
    } else {
        "remote unchanged"
    };
    let mut message = format!(
        "{} {}\n",
        styled("1;31", "\u{2717}"),
        styled("1;31", &format!("{headline} \u{b7} {}", kind.reason())),
    );
    message.push_str(&format!("  {url}\n"));
    if kind == Failure::Other
        && let Some(line) = error
            .detail()
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
    {
        message.push_str(&format!("  {}\n", styled("2", line)));
    }
    message.push_str(&format!("  {} {}\n", styled("36", "\u{2192}"), kind.fix()));
    let trailer = if cleared {
        "sync stays off (previous remote cleared)"
    } else {
        "sync unchanged (previous remote kept)"
    };
    message.push_str(&format!("  {}", styled("2", trailer)));
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
    use super::{env_shadow_warning, normalize_remote, unreachable};
    use crate::sync::Error;

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
        let message = unreachable(url, &Error::new("remote: Repository not found."), true);
        assert!(
            message.starts_with("\u{2717} remote not set \u{b7} repository not found"),
            "{message}"
        );
        assert_eq!(message.matches(url).count(), 1, "{message}");
    }

    #[test]
    fn a_transient_probe_failure_keeps_the_previous_remote() {
        let url = "https://github.com/mancuoj/tasu-data.git";
        let message = unreachable(
            url,
            &Error::new("fatal: unable to access: could not resolve host: github.com"),
            false,
        );
        assert!(
            message.starts_with("\u{2717} remote unchanged \u{b7} can't reach the host"),
            "{message}"
        );
        assert!(message.contains("previous remote kept"), "{message}");
    }

    #[test]
    fn the_shadow_warning_names_the_env_remote() {
        let warning = env_shadow_warning("git@env:repo.git");
        assert!(warning.contains("git@env:repo.git"), "{warning}");
        assert!(warning.contains("no effect"), "{warning}");
    }
}
