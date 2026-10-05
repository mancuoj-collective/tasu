use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Local;

use crate::config::Config;
use crate::domain::{Board, Bucket, settle};
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
    println!("+ {title}");

    if let Err(err) = sync::commit_now(&config.data_dir, config.remote.as_deref()) {
        eprintln!("tasu: push failed: {err}");
    }
    Ok(())
}

/// `tasu list`: print the open tasks, grouped by bucket. Read-only and local —
/// run `tasu sync` first if you want the latest from the remote.
pub fn list(config: &Config) -> Result<()> {
    let mut board = Store::new(config.board_path()).load();
    settle(&mut board, Local::now());
    print!("{}", render_list(&board));
    Ok(())
}

fn render_list(board: &Board) -> String {
    let mut out = String::new();
    for bucket in [Bucket::Today, Bucket::Week, Bucket::Later] {
        let indices = board.open_in(bucket);
        if indices.is_empty() {
            continue;
        }
        out.push_str(bucket_label(bucket));
        out.push('\n');
        for index in indices {
            if let Some(task) = board.task(index) {
                out.push_str("  \u{25cb} ");
                out.push_str(&task.title);
                out.push('\n');
            }
        }
    }
    if out.is_empty() {
        out.push_str("no open tasks\n");
    }
    out
}

fn bucket_label(bucket: Bucket) -> &'static str {
    match bucket {
        Bucket::Today => "TODAY",
        Bucket::Week => "THIS WEEK",
        Bucket::Later => "LATER",
    }
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

/// `tasu update`: upgrade using however tasu was installed, so the binary and
/// the package manager that owns it never disagree.
pub fn update() -> Result<()> {
    let method = Install::detect();
    println!(
        "tasu {} (installed via {})",
        env!("CARGO_PKG_VERSION"),
        method.label()
    );
    match method {
        Install::Homebrew => run("brew", &["upgrade", TAP_FORMULA]),
        Install::Installer => {
            if cfg!(windows) {
                print!("{}", windows_installer_hint());
                Ok(())
            } else {
                run("sh", &["-c", &installer_pipe()])
            }
        }
        Install::Cargo => {
            // Windows cannot overwrite a running executable, so point at the
            // command instead of failing halfway through.
            if cfg!(windows) {
                println!("run this in a new terminal:");
                println!("  cargo install tasu --force");
                Ok(())
            } else {
                run("cargo", &["install", "tasu", "--force"])
            }
        }
        Install::Unknown => {
            println!("could not tell how tasu was installed; update it the way you installed it:");
            println!("  Homebrew         brew upgrade {TAP_FORMULA}");
            println!("  Shell installer  {}", installer_pipe());
            print!("{}", windows_installer_hint());
            println!("  Cargo            cargo install tasu --force");
            Ok(())
        }
    }
}

const TAP_FORMULA: &str = "mancuoj/tap/tasu";
const INSTALL_URL: &str = "https://github.com/mancuoj-collective/tasu/releases/latest/download";

fn installer_pipe() -> String {
    format!("curl --proto '=https' --tlsv1.2 -LsSf {INSTALL_URL}/tasu-installer.sh | sh")
}

fn windows_installer_hint() -> String {
    format!(
        "  Windows          powershell -ExecutionPolicy Bypass -c \"irm {INSTALL_URL}/tasu-installer.ps1 | iex\"\n"
    )
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

/// How the running tasu was installed, inferred from where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Install {
    Homebrew,
    Installer,
    Cargo,
    Unknown,
}

impl Install {
    fn detect() -> Self {
        let exe = std::env::current_exe().ok();
        let target = exe
            .as_deref()
            .and_then(|path| std::fs::read_link(path).ok());
        let has_receipt = receipt_path().is_some_and(|path| path.exists());
        Self::classify(exe.as_deref(), target.as_deref(), has_receipt)
    }

    /// Pure, so it can be tested without a particular executable.
    fn classify(exe: Option<&Path>, symlink_target: Option<&Path>, has_receipt: bool) -> Self {
        let paths: Vec<&Path> = exe.into_iter().chain(symlink_target).collect();
        let has = |needle: &str| paths.iter().any(|p| p.to_string_lossy().contains(needle));
        if has("Cellar") {
            Self::Homebrew
        } else if has_receipt {
            Self::Installer
        } else if has(".cargo/bin") || has(".cargo\\bin") {
            Self::Cargo
        } else {
            Self::Unknown
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Installer => "the installer",
            Self::Cargo => "Cargo",
            Self::Unknown => "an unknown method",
        }
    }
}

/// The install receipt the shell / PowerShell installer writes.
fn receipt_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?
    };
    Some(base.join("tasu").join("tasu-receipt.json"))
}

/// Run an updater and inherit its output, so the user sees exactly what the
/// package manager did.
fn run(program: &str, args: &[&str]) -> Result<()> {
    println!("$ {program} {}", args.join(" "));
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => anyhow::bail!("`{program}` failed ({status}); tasu was not updated"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("`{program}` was not found on PATH")
        }
        Err(err) => Err(err).with_context(|| format!("failed to run `{program}`")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Install, normalize_remote};

    #[test]
    fn install_method_is_inferred_from_the_executable_path() {
        let cellar = Path::new("/opt/homebrew/Cellar/tasu/0.6.0/bin/tasu");
        assert_eq!(
            Install::classify(Some(cellar), None, false),
            Install::Homebrew
        );
        // A symlink at .../bin resolves into Cellar.
        assert_eq!(
            Install::classify(
                Some(Path::new("/opt/homebrew/bin/tasu")),
                Some(cellar),
                false
            ),
            Install::Homebrew
        );
        // The installer drops a receipt next to a ~/.cargo/bin binary.
        assert_eq!(
            Install::classify(Some(Path::new("/home/u/.cargo/bin/tasu")), None, true),
            Install::Installer
        );
        assert_eq!(
            Install::classify(Some(Path::new("/home/u/.cargo/bin/tasu")), None, false),
            Install::Cargo
        );
        assert_eq!(
            Install::classify(Some(Path::new("/usr/local/bin/tasu")), None, false),
            Install::Unknown
        );
    }

    #[test]
    fn list_prints_open_tasks_by_bucket() {
        use crate::domain::Board;
        use crate::domain::test_time::at;

        let mut board = Board::new();
        board.add("today one", at(2026, 10, 5));
        board.add("later one", at(2026, 10, 5));
        board.move_bucket(1, 2, at(2026, 10, 5));

        assert_eq!(
            super::render_list(&board),
            "TODAY\n  \u{25cb} today one\nLATER\n  \u{25cb} later one\n"
        );
    }

    #[test]
    fn list_reports_an_empty_board() {
        use crate::domain::Board;
        assert_eq!(super::render_list(&Board::new()), "no open tasks\n");
    }

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
