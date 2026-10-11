//! The git process boundary. Every invocation goes through here, so the C
//! locale, the deadlines and the shape of the error text are decided in one
//! place.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::Error;

/// A git command with the C locale forced, so git's messages are stable English:
/// tasu classifies them, and a translated git must never change behaviour.
pub(super) fn program() -> Command {
    let mut cmd = Command::new("git");
    cmd.env("LC_ALL", "C").env("LANG", "C");
    cmd
}

/// A git command run inside `repo` (`git -C <repo> ...`).
pub(super) fn in_repo(repo: &Path, args: &[&str]) -> Command {
    let mut cmd = program();
    cmd.arg("-C").arg(repo).args(args);
    cmd
}

/// Run a git command in `repo`. On failure the error is git's stderr, or its
/// stdout when stderr is empty.
pub(super) fn run(repo: &Path, args: &[&str]) -> Result<(), Error> {
    let output = in_repo(repo, args)
        .output()
        .map_err(|err| Error::new(err.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let mut message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.is_empty() {
        message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    Err(Error::new(message))
}

/// Whether a git command in `repo` exits successfully. Decided by exit status,
/// never by parsing output.
pub(super) fn ok(repo: &Path, args: &[&str]) -> bool {
    in_repo(repo, args)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Raw stdout of a git command in `repo`, when it succeeds.
pub(super) fn bytes(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = in_repo(repo, args).output().ok()?;
    output.status.success().then_some(output.stdout)
}

/// Trimmed stdout of a git command in `repo`, when it succeeds.
pub(super) fn text(repo: &Path, args: &[&str]) -> Option<String> {
    let bytes = bytes(repo, args)?;
    Some(String::from_utf8_lossy(&bytes).trim().to_string())
}

/// Run a network-bound command with a deadline, killing it if it overruns, and
/// return its stdout. Only for small outputs (`ls-remote`, `push --quiet`), since
/// the pipes are read after the process exits.
pub(super) fn bounded(cmd: &mut Command, timeout: Duration) -> Result<String, Error> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| Error::new(err.to_string()))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut stderr);
                }
                if status.success() {
                    return Ok(stdout);
                }
                let message = stderr.trim();
                return Err(Error::new(if message.is_empty() {
                    "git command failed".to_string()
                } else {
                    message.to_string()
                }));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::new("git command timed out"));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(Error::new(err.to_string())),
        }
    }
}
