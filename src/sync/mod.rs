use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Network operations are bounded so a hung connection cannot freeze a caller.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
const PUSH_TIMEOUT: Duration = Duration::from_secs(8);

/// Git-backed sync for a single writer.
///
/// The data directory *is* the repository working tree. All git work happens on
/// a worker thread; the app talks to it through a channel and is never blocked
/// by the network. Without a remote, nothing here ever shells out to git.
pub struct Sync {
    repo: PathBuf,
    remote: Option<String>,
    jobs: Sender<Job>,
    results: Receiver<Result<(), String>>,
}

enum Job {
    Pull,
    CommitPush,
}

impl Sync {
    pub fn new(repo: PathBuf, remote: Option<String>) -> Self {
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (results_tx, results_rx) = mpsc::channel::<Result<(), String>>();

        let worker_repo = repo.clone();
        let worker_remote = remote.clone();
        thread::spawn(move || {
            // Adopt or create the repository once, before the first job.
            if ensure_repo(&worker_repo, worker_remote.as_deref()).is_err() {
                // Fall through: local-only operation still works.
            }
            while let Ok(job) = jobs_rx.recv() {
                let result = match job {
                    Job::Pull => pull(&worker_repo, worker_remote.as_deref()),
                    Job::CommitPush => commit_push(&worker_repo, worker_remote.as_deref()),
                };
                if results_tx.send(result).is_err() {
                    break;
                }
            }
        });

        Self {
            repo,
            remote,
            jobs: jobs_tx,
            results: results_rx,
        }
    }

    /// Fetch the latest remote state; the changed file is picked up by the
    /// mtime watcher, which reloads the board.
    pub fn pull(&self) -> Result<(), String> {
        self.jobs
            .send(Job::Pull)
            .map_err(|_| "sync worker stopped".to_string())
    }

    /// Stage, commit and push. Debounced by the caller.
    pub fn commit_push(&self) -> Result<(), String> {
        self.jobs
            .send(Job::CommitPush)
            .map_err(|_| "sync worker stopped".to_string())
    }

    pub fn poll(&self) -> Option<Result<(), String>> {
        self.results.try_recv().ok()
    }

    /// Best-effort push on exit, run on the calling thread so it is not lost
    /// when the process ends.
    pub fn flush(&self) -> Result<(), String> {
        commit_now(&self.repo, self.remote.as_deref())
    }
}

/// One synchronous commit+push, for one-shot commands and process exit.
pub fn commit_now(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    ensure_repo(repo, remote)?;
    commit_push(repo, remote)
}

/// One synchronous pull, used before a one-shot command reads the board so a
/// CLI-only machine does not diverge from the remote.
pub fn pull_now(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    ensure_repo(repo, remote)?;
    pull(repo, remote)
}

/// Check that a remote is reachable and that credentials work, without asking
/// for input. Empty repositories count as reachable.
pub fn probe_remote(url: &str) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.args(["ls-remote", url]);
    run_bounded(&mut cmd, NETWORK_TIMEOUT)
}

fn ensure_repo(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if repo.join(".git").exists() {
        ensure_gitignore(repo);
        if let Some(url) = remote {
            // set-url works on an existing origin (unlike `remote add`, which
            // silently fails and leaves a stale URL).
            if git(repo, &["remote", "set-url", "origin", url]).is_err() {
                let _ = git(repo, &["remote", "add", "origin", url]);
            }
        }
        return Ok(());
    }

    // Without a remote tasu is purely local and never shells out to git.
    let Some(url) = remote else {
        return Ok(());
    };

    std::fs::create_dir_all(repo).map_err(|err| err.to_string())?;

    // An empty directory is the second machine: clone the history.
    let empty = std::fs::read_dir(repo)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true);
    if empty && clone(url, repo).is_ok() {
        ensure_gitignore(repo);
        return Ok(());
    }

    // Existing local data (first machine, or a remote that already has a
    // README). We keep the local board aside, adopt the remote's history, then
    // replay the board on top and push. No unrelated-history merge, which proved
    // unreliable across git builds.
    let board = std::fs::read(repo.join("todos.json")).ok();

    git(repo, &["init", "-q"])?;
    let _ = git(repo, &["remote", "add", "origin", url]);

    // Fetch the remote's current tip into a private ref, independent of
    // remote-tracking branches (which are not always created), then adopt its
    // tree as the base.
    if git(
        repo,
        &["fetch", "--quiet", "origin", "+HEAD:refs/tasu/remote"],
    )
    .is_ok()
    {
        let _ = git(repo, &["reset", "--hard", "refs/tasu/remote"]);
    }

    // Replay the local board and commit it on top (or as the root commit).
    if let Some(bytes) = board {
        std::fs::write(repo.join("todos.json"), bytes).map_err(|err| err.to_string())?;
    }
    ensure_gitignore(repo);
    let _ = git(repo, &["add", "-A"]);
    let _ = git(
        repo,
        &[
            "-c",
            "user.name=tasu",
            "-c",
            "user.email=tasu@localhost",
            "commit",
            "--quiet",
            "-m",
            "tasu: local",
        ],
    );
    Ok(())
}

/// Keep machine-local files out of the synced repository. On macOS and Windows
/// the config directory equals the data directory, so `config.json` would
/// otherwise be committed and pushed.
fn ensure_gitignore(repo: &Path) {
    let path = repo.join(".gitignore");
    if !path.exists() {
        let _ = std::fs::write(&path, "config.json\n*.tmp\n*.corrupt-*\n");
    }
}

fn clone(url: &str, repo: &Path) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--quiet", url]).arg(repo);
    run_bounded(&mut cmd, NETWORK_TIMEOUT)
}

fn pull(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if remote.is_none() {
        return Ok(());
    }
    let mut cmd = git_cmd(repo, &["pull", "--rebase", "--autostash", "--quiet"]);
    run_bounded(&mut cmd, NETWORK_TIMEOUT)
}

fn commit_push(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if remote.is_none() {
        return Ok(());
    }
    ensure_gitignore(repo);
    git(repo, &["add", "-A"])?;
    match git(
        repo,
        &[
            "-c",
            "user.name=tasu",
            "-c",
            "user.email=tasu@localhost",
            "commit",
            "--quiet",
            "-m",
            "tasu: sync",
        ],
    ) {
        Ok(()) => {}
        Err(message) if message.contains("nothing to commit") => {}
        Err(message) => return Err(message),
    }
    let mut cmd = git_cmd(repo, &["push", "--quiet", "-u", "origin", "HEAD"]);
    run_bounded(&mut cmd, PUSH_TIMEOUT)?;
    Ok(())
}

fn git_cmd(repo: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    cmd
}

fn git(repo: &Path, args: &[&str]) -> Result<(), String> {
    let output = git_cmd(repo, args)
        .output()
        .map_err(|err| err.to_string())?;
    check(output)
}

/// Run a command with a deadline, killing it if it overruns. Used for the
/// network-bound git operations so nothing can hang indefinitely.
fn run_bounded(cmd: &mut Command, timeout: Duration) -> Result<(), String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut message);
                }
                let message = message.trim();
                return Err(if message.is_empty() {
                    "git command failed".to_string()
                } else {
                    message.to_string()
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("git command timed out".to_string());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}

fn check(output: std::process::Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let mut message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.is_empty() {
        message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A bare repository standing in for the remote, so the tests exercise the
    /// real git binary end to end instead of a mock.
    fn bare_remote(dir: &Path) -> String {
        let path = dir.join("remote.git");
        let status = Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "failed to create bare remote");
        path.to_string_lossy().into_owned()
    }

    fn remote_refs(remote: &str) -> String {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(remote)
            .args(["for-each-ref", "--format=%(refname)"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn pushes_a_local_board_to_the_remote() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"version\":1,\"tasks\":[]}").unwrap();

        let remote = bare_remote(dir.path());
        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();

        assert!(
            remote_refs(&remote).contains("refs/heads/"),
            "remote received no branch"
        );
    }

    #[test]
    fn second_machine_clones_instead_of_init() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::write(first.join("todos.json"), "{\"version\":1,\"tasks\":[]}").unwrap();
        let remote = bare_remote(dir.path());
        ensure_repo(&first, Some(&remote)).unwrap();
        commit_push(&first, Some(&remote)).unwrap();

        // Fresh machine: empty directory, same remote.
        let second = dir.path().join("second");
        ensure_repo(&second, Some(&remote)).unwrap();

        assert!(second.join(".git").exists(), "did not become a repo");
        assert!(
            second.join("todos.json").exists(),
            "did not clone the board"
        );
    }

    #[test]
    fn grafts_onto_a_remote_that_already_has_history() {
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

        // Seed the remote with a README commit, like GitHub's "add a README".
        let seed = dir.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        std::fs::write(seed.join("README.md"), "# tasu data\n").unwrap();
        git(&seed, &["init", "-q"]).unwrap();
        git(&seed, &["add", "-A"]).unwrap();
        git(
            &seed,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "seed",
            ],
        )
        .unwrap();
        git(&seed, &["remote", "add", "origin", remote.as_str()]).unwrap();
        git(&seed, &["push", "-q", "-u", "origin", "HEAD"]).unwrap();

        // First machine with existing local data and no repository yet.
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"version\":1,\"tasks\":[]}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();

        let tree = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["ls-tree", "-r", "--name-only", "HEAD"])
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&tree.stdout);
        assert!(
            files.contains("README.md"),
            "remote history was lost: {files}"
        );
        assert!(files.contains("todos.json"), "local board was not pushed");
    }

    #[test]
    fn without_a_remote_nothing_breaks() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{}").unwrap();

        ensure_repo(&data, None).unwrap();
        commit_push(&data, None).unwrap();
        assert!(
            !data.join(".git").exists(),
            "local-only must not create a repository"
        );
    }

    #[test]
    fn config_file_is_not_synced() {
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"version\":1,\"tasks\":[]}").unwrap();
        // On macOS/Windows the config lives in the data directory.
        std::fs::write(data.join("config.json"), "{\"remote\":\"x\"}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();

        let tree = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["ls-tree", "-r", "--name-only", "HEAD"])
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&tree.stdout);
        assert!(files.contains("todos.json"), "board missing: {files}");
        assert!(
            !files.contains("config.json"),
            "machine-local config must not be synced: {files}"
        );
    }

    #[test]
    fn changing_the_remote_updates_origin() {
        let dir = tempfile::tempdir().unwrap();
        let first = bare_remote(dir.path());
        let second = dir.path().join("second.git");
        assert!(
            Command::new("git")
                .args(["init", "--bare", "--quiet"])
                .arg(&second)
                .status()
                .unwrap()
                .success()
        );
        let second = second.to_string_lossy().into_owned();

        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"version\":1,\"tasks\":[]}").unwrap();
        ensure_repo(&data, Some(&first)).unwrap();
        ensure_repo(&data, Some(&second)).unwrap();

        let out = Command::new("git")
            .arg("-C")
            .arg(&data)
            .args(["remote", "get-url", "origin"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), second);
    }
}
