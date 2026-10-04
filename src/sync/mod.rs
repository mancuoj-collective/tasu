use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

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

fn ensure_repo(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if repo.join(".git").exists() {
        if let Some(url) = remote {
            let _ = git(repo, &["remote", "add", "origin", url]);
        }
        return Ok(());
    }

    std::fs::create_dir_all(repo).map_err(|err| err.to_string())?;

    if let Some(url) = remote {
        // Prefer cloning when the data directory is still empty: this is how a
        // second machine adopts the existing history.
        let empty = std::fs::read_dir(repo)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if empty && clone(url, repo).is_ok() {
            return Ok(());
        }
    }

    git(repo, &["init", "-q"])?;
    if let Some(url) = remote {
        let _ = git(repo, &["remote", "add", "origin", url]);
    }
    Ok(())
}

fn clone(url: &str, repo: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["clone", "--quiet", url])
        .arg(repo)
        .output()
        .map_err(|err| err.to_string())?;
    check(output)
}

fn pull(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if remote.is_none() {
        return Ok(());
    }
    git(repo, &["pull", "--rebase", "--autostash", "--quiet"])
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
        git(repo, &["push", "--quiet", "-u", "origin", "HEAD"])?;
    }
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    check(output)
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
