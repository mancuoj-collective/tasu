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
    pub fn pull(&self) {
        let _ = self.jobs.send(Job::Pull);
    }

    /// Stage, commit and push. Debounced by the caller.
    pub fn commit_push(&self) {
        let _ = self.jobs.send(Job::CommitPush);
    }

    pub fn poll(&self) -> Option<Result<(), String>> {
        self.results.try_recv().ok()
    }

    /// Best-effort push on exit, run on the calling thread so it is not lost
    /// when the process ends.
    pub fn flush(&self) {
        let _ = commit_now(&self.repo, self.remote.as_deref());
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
        if let Some(url) = remote {
            let _ = git(repo, &["remote", "add", "origin", url]);
        }
        return Ok(());
    }

    std::fs::create_dir_all(repo).map_err(|err| err.to_string())?;

    if let Some(url) = remote {
        // An empty directory is the second machine: clone the history.
        let empty = std::fs::read_dir(repo)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if empty && clone(url, repo).is_ok() {
            return Ok(());
        }

        // Existing local data (first machine, or a remote that already has a
        // README): init, commit the local board, then merge the remote's
        // unrelated history, preferring local content on conflicts. This keeps
        // remote-only files instead of deleting them on the next `add -A`.
        git(repo, &["init", "-q"])?;
        let _ = git(repo, &["remote", "add", "origin", url]);
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
        if git(repo, &["fetch", "--quiet", "origin"]).is_ok()
            && let Some(target) = remote_branch(repo)
        {
            let _ = git(
                repo,
                &[
                    "merge",
                    "--allow-unrelated-histories",
                    "-X",
                    "ours",
                    "--quiet",
                    "-m",
                    "tasu: merge",
                    &target,
                ],
            );
        }
        return Ok(());
    }

    git(repo, &["init", "-q"])?;
    Ok(())
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
    if remote.is_some() {
        let mut cmd = git_cmd(repo, &["push", "--quiet", "-u", "origin", "HEAD"]);
        run_bounded(&mut cmd, PUSH_TIMEOUT)?;
    }
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

fn git_stdout(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_cmd(repo, args)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Pick a fetched remote branch to merge into the local history: the one
/// matching the local branch when possible, else the first. Avoids relying on
/// `origin/HEAD`, which some git setups do not maintain.
fn remote_branch(repo: &Path) -> Option<String> {
    let local = git_stdout(repo, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_default();
    let refs = git_stdout(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin",
        ],
    )
    .unwrap_or_default();

    let mut branches: Vec<String> = refs
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.ends_with("/HEAD"))
        .map(str::to_string)
        .collect();

    let preferred = format!("origin/{}", local.trim());
    if let Some(position) = branches.iter().position(|branch| *branch == preferred) {
        return Some(branches.remove(position));
    }
    branches.into_iter().next()
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
        assert!(data.join(".git").exists());
    }
}
