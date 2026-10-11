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
        .map_err(|err| Error::io(err.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(Error::new(git_message(&output)))
}

/// The human-facing text of a failed git command: stderr, or stdout when stderr
/// is empty, or a generic line when both are.
fn git_message(output: &std::process::Output) -> String {
    let mut message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.is_empty() {
        message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    if message.is_empty() {
        "git command failed".to_string()
    } else {
        message
    }
}

/// Whether a git command in `repo` exits successfully. Decided by exit status,
/// never by parsing output. Only for queries where "false" is a safe answer.
pub(super) fn ok(repo: &Path, args: &[&str]) -> bool {
    in_repo(repo, args)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether `ancestor` is an ancestor of `descendant`. Exit 1 means "no"; any
/// other non-zero code — a missing object, a corrupt repository — is a real git
/// failure and is returned as an error rather than read as "no".
pub(super) fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool, Error> {
    let output = in_repo(repo, &["merge-base", "--is-ancestor", ancestor, descendant])
        .output()
        .map_err(|err| Error::io(err.to_string()))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(Error::new(git_message(&output))),
    }
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
/// return its stdout. The pipes are drained on their own threads while the
/// deadline is enforced, so a command that writes more than the pipe buffer
/// cannot block on the write and be killed as if it had timed out.
pub(super) fn bounded(cmd: &mut Command, timeout: Duration) -> Result<String, Error> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| Error::io(err.to_string()))?;

    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let out_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let err_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = out_reader.join().unwrap_or_default();
                let stderr = err_reader.join().unwrap_or_default();
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
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(Error::new("git command timed out"));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(Error::io(err.to_string())),
        }
    }
}
