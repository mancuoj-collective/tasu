use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Network operations are bounded so a hung connection cannot freeze a caller.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
const PUSH_TIMEOUT: Duration = Duration::from_secs(8);

/// Branch used when the remote has no branch to adopt. A fixed name keeps every
/// machine on the same branch: the host's `init.defaultBranch` (often `master`
/// on Windows) must never leak into the data repository.
const DEFAULT_BRANCH: &str = "main";

/// Git-backed sync for a single writer.
///
/// The data directory *is* the repository working tree. All git work happens on
/// a worker thread; the app talks to it through a channel and is never blocked
/// by the network. Without a remote, nothing here ever shells out to git.
pub struct Sync {
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

        // The worker owns the repo and remote; the app only talks to it over
        // the channels.
        thread::spawn(move || {
            // Adopt or create the repository once, before the first job.
            if ensure_repo(&repo, remote.as_deref()).is_err() {
                // Fall through: local-only operation still works.
            }
            while let Ok(job) = jobs_rx.recv() {
                let result = match job {
                    Job::Pull => pull(&repo, remote.as_deref()),
                    Job::CommitPush => commit_push(&repo, remote.as_deref()),
                };
                if results_tx.send(result).is_err() {
                    break;
                }
            }
        });

        Self {
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

/// A short, human-readable reason a sync failed, inferred from git's stderr.
/// Used to lead with the likely cause instead of a raw error dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NotFound,
    Auth,
    Network,
    Other,
}

impl Failure {
    pub fn classify(error: &str) -> Self {
        let error = error.to_ascii_lowercase();
        let has = |needle: &str| error.contains(needle);
        if has("repository not found")
            || has("does not appear to be a git repository")
            || (has("repository") && has("not found"))
        {
            Self::NotFound
        } else if has("authentication failed")
            || has("could not read username")
            || has("could not read password")
            || has("terminal prompts disabled")
            || has("permission denied")
            || has("403")
        {
            Self::Auth
        } else if has("could not resolve host")
            || has("could not resolve")
            || has("unable to access")
            || has("failed to connect")
            || has("timed out")
            || has("network is unreachable")
        {
            Self::Network
        } else {
            Self::Other
        }
    }

    /// A phrase naming the cause, for the `tasu remote` failure message.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotFound => "repository not found",
            Self::Auth => "authentication failed",
            Self::Network => "can't reach the host",
            Self::Other => "sync failed",
        }
    }

    /// A phrase for the footer, paired with a `tasu sync` hint.
    pub fn footer(self) -> &'static str {
        match self {
            Self::NotFound => "remote not found",
            Self::Auth => "auth failed",
            Self::Network => "offline",
            Self::Other => "sync failed",
        }
    }

    /// One actionable line, for the `tasu remote` failure message.
    pub fn fix(self) -> &'static str {
        match self {
            Self::NotFound => {
                "check the owner/repo, or create the repository (private is fine), then retry"
            }
            Self::Auth => {
                "no stored credentials or no write access \u{2014} over HTTPS git needs a token, or use an SSH remote"
            }
            Self::Network => "check your network and the host name, then retry",
            Self::Other => "the git error is above",
        }
    }
}

fn ensure_repo(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if repo.join(".git").exists() {
        ensure_gitignore(repo);
        if let Some(url) = remote {
            set_origin(repo, url);
            // All machines must agree on one branch. Reconcile it once per
            // remote, then stop asking: the `ls-remote` this needs is a network
            // round-trip we do not want on every launch. An unreachable remote
            // is not marked, so it is retried next time.
            if !is_settled(repo, url)
                && let Ok(branch) = remote_default_branch_result(url)
            {
                if let Some(branch) = branch {
                    name_branch(repo, &branch);
                }
                mark_settled(repo, url);
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
        // Check out first: an untracked `.gitignore` written before the
        // checkout would block git from creating the tracked one, leaving the
        // board unpopulated.
        settle_head(repo, &target_branch(url));
        ensure_gitignore(repo);
        mark_settled(repo, url);
        return Ok(());
    }

    // Existing local data (first machine, or a remote that already has a
    // README). The local board is kept aside, the remote's history is adopted,
    // then the board is committed on top and pushed. No unrelated-history merge,
    // which proved unreliable across git builds.
    let local_board = std::fs::read(repo.join("todos.json")).ok();

    // Name the branch before the first commit: `git init` alone would use the
    // host's default (e.g. `master`), splitting the remote into two branches.
    let branch = target_branch(url);
    git(repo, &["init", "-q"])?;
    name_branch(repo, &branch);
    let _ = git(repo, &["remote", "add", "origin", url]);

    // Fetch the remote's current tip into a private ref, independent of
    // remote-tracking branches (which are not always created), then adopt its
    // tree as the base.
    let fetched = git(
        repo,
        &[
            "fetch",
            "--quiet",
            "origin",
            &format!("+refs/heads/{branch}:refs/tasu/remote"),
        ],
    )
    .is_ok()
        || git(
            repo,
            &["fetch", "--quiet", "origin", "+HEAD:refs/tasu/remote"],
        )
        .is_ok();
    if fetched {
        let _ = git(repo, &["reset", "--hard", "refs/tasu/remote"]);
    }

    // The remote may already carry a board. When both sides have one, union them
    // so connecting a remote never silently drops the other machine's tasks;
    // otherwise keep whichever side has data.
    let remote_board = if fetched {
        git_bytes(repo, &["show", "refs/tasu/remote:todos.json"])
    } else {
        None
    };
    let board = match (local_board, remote_board) {
        (Some(local), Some(remote)) => crate::store::merge_files(&local, &remote).or(Some(local)),
        (board, None) => board,
        (None, board) => board,
    };
    if let Some(bytes) = board.filter(|bytes| !bytes.is_empty()) {
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
    mark_settled(repo, url);
    Ok(())
}

/// Point `origin` at `url`. `set-url` works on an existing origin, unlike
/// `remote add`, which silently fails and leaves a stale URL.
fn set_origin(repo: &Path, url: &str) {
    if git(repo, &["remote", "set-url", "origin", url]).is_err() {
        let _ = git(repo, &["remote", "add", "origin", url]);
    }
}

/// The branch tasu syncs on: the remote's default branch when it already has
/// one, otherwise [`DEFAULT_BRANCH`].
fn target_branch(remote: &str) -> String {
    remote_default_branch(remote).unwrap_or_else(|| DEFAULT_BRANCH.to_string())
}

/// The remote's default branch, read from its symbolic `HEAD`. `None` when the
/// remote is empty, unreachable, or has an unusual layout.
fn remote_default_branch(remote: &str) -> Option<String> {
    remote_default_branch_result(remote).ok().flatten()
}

/// Like [`remote_default_branch`], but distinguishes an unreachable remote
/// (`Err`) from a readable one that simply has no default branch (`Ok(None)`).
/// `ensure_repo` uses the difference to decide whether to retry later.
fn remote_default_branch_result(remote: &str) -> Result<Option<String>, String> {
    let mut cmd = Command::new("git");
    cmd.args(["ls-remote", "--symref", remote, "HEAD"]);
    let output = run_output(&mut cmd, NETWORK_TIMEOUT)?;
    // "ref: refs/heads/main\tHEAD"
    Ok(output.lines().find_map(|line| {
        let name = line
            .strip_prefix("ref:")?
            .trim()
            .strip_prefix("refs/heads/")?;
        name.split_whitespace().next().map(str::to_string)
    }))
}

/// Remember the remote whose default branch we already reconciled, so later
/// runs skip the `ls-remote` that needs. Stored in the repo's own config, never
/// committed.
fn mark_settled(repo: &Path, url: &str) {
    let _ = git(repo, &["config", "tasu.remote", url]);
}

/// Whether the branch for `url` was already reconciled on a previous run.
fn is_settled(repo: &Path, url: &str) -> bool {
    git_string(repo, &["config", "--get", "tasu.remote"]).as_deref() == Some(url)
}

/// The current branch, or `None` on a detached HEAD or an error.
fn current_branch(repo: &Path) -> Option<String> {
    let output = git_cmd(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Make sure a freshly cloned repository is on a real branch. When the clone
/// checked something out, normalise the name; otherwise check out the remote
/// branch we expected, or any branch it does have.
fn settle_head(repo: &Path, target: &str) {
    if git_ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        name_branch(repo, target);
        return;
    }
    if git_ok(
        repo,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{target}"),
        ],
    ) {
        let _ = git(repo, &["checkout", "--quiet", target]);
    } else if let Some(branch) = first_remote_branch(repo) {
        let _ = git(repo, &["checkout", "--quiet", &branch]);
    } else {
        name_branch(repo, target);
    }
}

/// The first branch under `origin/`, for a remote whose symbolic `HEAD` is
/// stale. `None` when the remote has no branches.
fn first_remote_branch(repo: &Path) -> Option<String> {
    let output = git_cmd(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin/",
        ],
    )
    .output()
    .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "origin/HEAD")
        .filter_map(|name| name.strip_prefix("origin/"))
        .map(str::to_string)
        .next()
}

/// Rename the local branch to `target` (or name an unborn HEAD), so pushes land
/// on the branch every machine agreed on. A no-op when already correct.
fn name_branch(repo: &Path, target: &str) {
    let Some(current) = current_branch(repo) else {
        let _ = git(
            repo,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{target}")],
        );
        return;
    };
    if current == target {
        return;
    }
    if git_ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        // A born branch: `branch -m` moves the ref and HEAD together. If the
        // target already exists locally we cannot rename, so switch instead.
        if git(repo, &["branch", "-m", target]).is_ok() {
            eprintln!("tasu: sync branch {current} renamed to {target}");
        } else {
            let _ = git(repo, &["checkout", "--quiet", target]);
        }
    } else {
        // Unborn HEAD (fresh init or an empty clone): just point HEAD.
        let _ = git(
            repo,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{target}")],
        );
    }
}

/// Keep the transient files `Store` writes — a half-written temp, a corrupt
/// backup — out of the synced repository. Added idempotently.
fn ensure_gitignore(repo: &Path) {
    let path = repo.join(".gitignore");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut changed = false;
    for entry in ["*.tmp", "*.corrupt-*"] {
        if !text.lines().any(|line| line.trim() == entry) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(entry);
            text.push('\n');
            changed = true;
        }
    }
    if changed && let Err(err) = std::fs::write(&path, &text) {
        eprintln!("tasu: could not write {}: {err}", path.display());
    }
}

fn clone(url: &str, repo: &Path) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--quiet", url]).arg(repo);
    run_bounded(&mut cmd, NETWORK_TIMEOUT)
}

/// Bring the local branch up to date with the remote without ever producing a
/// rebase conflict. The board is the whole state, so history carries no extra
/// meaning; the local board is merged **task by task** into the remote's.
fn pull(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    if remote.is_none() {
        return Ok(());
    }
    // Nothing to pull until the first commit exists (fresh remote).
    if !git_ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return Ok(());
    }
    let Some(branch) = current_branch(repo) else {
        return Ok(());
    };

    // Fetch the remote branch into a private ref (remote-tracking refs are not
    // always created), then compare histories.
    let mut fetch = git_cmd(
        repo,
        &[
            "fetch",
            "--quiet",
            "origin",
            &format!("+refs/heads/{branch}:refs/tasu/remote"),
        ],
    );
    run_bounded(&mut fetch, NETWORK_TIMEOUT)?;

    // If the remote is already reachable from HEAD, we are level or ahead.
    if git_ok(
        repo,
        &["merge-base", "--is-ancestor", "refs/tasu/remote", "HEAD"],
    ) {
        return Ok(());
    }

    // The remote is ahead, or the two histories diverged. Rebase would conflict
    // on `todos.json`; instead adopt the remote's tree and overlay the local
    // board, merging task by task so nothing is lost and the repo never sticks.
    let local_board = std::fs::read(repo.join("todos.json")).ok();
    let remote_board = git_bytes(repo, &["show", "refs/tasu/remote:todos.json"]);
    let _ = git(repo, &["reset", "--hard", "refs/tasu/remote"]);
    let board = match (local_board, remote_board) {
        // Remote is the shared base; the local board fills in and advances.
        (Some(local), Some(remote)) => crate::store::merge_files(&remote, &local).or(Some(remote)),
        (Some(local), None) => Some(local),
        (None, board) => board,
    };
    if let Some(bytes) = board.filter(|bytes| !bytes.is_empty()) {
        std::fs::write(repo.join("todos.json"), bytes).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn commit_push(repo: &Path, remote: Option<&str>) -> Result<(), String> {
    commit_push_with(repo, remote, PUSH_TIMEOUT)
}

/// `commit_push` with an explicit push deadline, so tests can allow for a slow
/// machine without loosening the bound the app ships with.
fn commit_push_with(repo: &Path, remote: Option<&str>, timeout: Duration) -> Result<(), String> {
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
    // Push to the explicit branch name instead of a bare `HEAD`, so a stray
    // local branch can never create a second branch on the remote.
    let branch = current_branch(repo).unwrap_or_else(|| DEFAULT_BRANCH.to_string());
    let mut cmd = git_cmd(
        repo,
        &[
            "push",
            "--quiet",
            "-u",
            "origin",
            &format!("HEAD:refs/heads/{branch}"),
        ],
    );
    run_bounded(&mut cmd, timeout)?;
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

fn git_ok(repo: &Path, args: &[&str]) -> bool {
    git_cmd(repo, args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// stdout of a git command expected to succeed, for reading a file out of a
/// commit (`git show <ref>:<path>`).
fn git_bytes(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = git_cmd(repo, args).output().ok()?;
    output.status.success().then_some(output.stdout)
}

/// Trimmed stdout of a git command expected to succeed.
fn git_string(repo: &Path, args: &[&str]) -> Option<String> {
    let bytes = git_bytes(repo, args)?;
    Some(String::from_utf8_lossy(&bytes).trim().to_string())
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

/// Like [`run_bounded`], but returns stdout so a caller can parse it. Only for
/// small outputs (`ls-remote`), since stdout is read after the process exits.
fn run_output(cmd: &mut Command, timeout: Duration) -> Result<String, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

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

    /// Shadow the production `commit_push` with a generous deadline: CI —
    /// Windows especially — can be slow when these tests spawn many git
    /// processes in parallel.
    fn commit_push(repo: &Path, remote: Option<&str>) -> Result<(), String> {
        commit_push_with(repo, remote, Duration::from_secs(60))
    }
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

    /// Seed a bare remote with a commit on `branch` and point its `HEAD` there,
    /// the way a hosting provider marks a default branch.
    fn seeded_remote(dir: &Path, branch: &str) -> String {
        let path = dir.join("remote.git");
        assert!(
            Command::new("git")
                .args(["init", "--bare", "--quiet"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let url = path.to_string_lossy().into_owned();

        let seed = dir.join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        std::fs::write(seed.join("README.md"), "# tasu data\n").unwrap();
        git(&seed, &["init", "-q", "-b", branch]).unwrap();
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
        git(&seed, &["remote", "add", "origin", &url]).unwrap();
        git(&seed, &["push", "-q", "origin", branch]).unwrap();
        let _ = Command::new("git")
            .arg("--git-dir")
            .arg(&path)
            .args(["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")])
            .status();
        url
    }

    #[test]
    fn pushes_a_local_board_to_the_remote() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

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
        std::fs::write(first.join("todos.json"), "{\"tasks\":[]}").unwrap();
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
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

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
    fn graft_follows_the_remote_default_branch() {
        let dir = tempfile::tempdir().unwrap();
        let remote = seeded_remote(dir.path(), "trunk");

        // First machine with local data but no repository yet. The host's
        // `init.defaultBranch` must not decide the branch name.
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();

        assert_eq!(current_branch(&data).as_deref(), Some("trunk"));
        let refs = remote_refs(&remote);
        assert!(refs.contains("refs/heads/trunk"), "expected trunk: {refs}");
        assert!(!refs.contains("refs/heads/master"), "stray branch: {refs}");
        assert!(!refs.contains("refs/heads/main"), "stray branch: {refs}");
    }

    /// Write a board with explicit `(title, day)` tasks, so a test controls the
    /// creation times that are the task identity.
    fn write_board(dir: &Path, tasks: &[(&str, u32)]) {
        let mut board = crate::domain::Board::new();
        for (title, day) in tasks {
            board.add(*title, crate::domain::test_time::at(2026, 10, *day));
        }
        crate::store::Store::new(dir.join("todos.json"))
            .save(&board)
            .unwrap();
    }

    fn commit(repo: &Path, message: &str) {
        git(repo, &["add", "-A"]).unwrap();
        git(
            repo,
            &[
                "-c",
                "user.name=tasu",
                "-c",
                "user.email=tasu@localhost",
                "commit",
                "-q",
                "-m",
                message,
            ],
        )
        .unwrap();
    }

    fn add_task(repo: &Path, title: &str, day: u32) {
        let store = crate::store::Store::new(repo.join("todos.json"));
        let mut board = store.load();
        board.add(title, crate::domain::test_time::at(2026, 10, day));
        store.save(&board).unwrap();
    }

    fn clone_repo(remote: &str, into: &Path) {
        let status = Command::new("git")
            .args(["clone", "--quiet", remote])
            .arg(into)
            .status()
            .unwrap();
        assert!(status.success(), "clone failed");
    }

    #[test]
    fn connecting_to_a_remote_that_has_a_board_unions_both() {
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

        // First machine publishes a board.
        let first = dir.path().join("first");
        std::fs::create_dir_all(&first).unwrap();
        write_board(&first, &[("remote task", 5)]);
        ensure_repo(&first, Some(&remote)).unwrap();
        commit_push(&first, Some(&remote)).unwrap();

        // Second machine already has its own board, then enables the remote.
        let second = dir.path().join("second");
        std::fs::create_dir_all(&second).unwrap();
        write_board(&second, &[("local task", 6)]);
        ensure_repo(&second, Some(&remote)).unwrap();
        commit_push(&second, Some(&remote)).unwrap();

        let board = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["show", "refs/heads/main:todos.json"])
            .output()
            .unwrap();
        let board = String::from_utf8_lossy(&board.stdout);
        assert!(
            board.contains("remote task"),
            "lost the remote board: {board}"
        );
        assert!(
            board.contains("local task"),
            "lost the local board: {board}"
        );
    }

    #[test]
    fn a_fresh_remote_gets_the_fixed_branch_not_the_host_default() {
        let dir = tempfile::tempdir().unwrap();
        // Empty remote: nothing to adopt, so tasu picks the fixed name.
        let remote = bare_remote(dir.path());
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();

        assert_eq!(current_branch(&data).as_deref(), Some("main"));
        let refs = remote_refs(&remote);
        assert!(refs.contains("refs/heads/main"), "expected main: {refs}");
        assert!(!refs.contains("refs/heads/master"), "stray branch: {refs}");
    }

    #[test]
    fn an_existing_repo_is_reconciled_with_the_remote_branch() {
        let dir = tempfile::tempdir().unwrap();
        // The remote lives on `main`, but this machine's repo was created under
        // a git whose default branch was `master`.
        let remote = seeded_remote(dir.path(), "main");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();
        git(&data, &["init", "-q"]).unwrap();
        git(&data, &["add", "-A"]).unwrap();
        git(
            &data,
            &[
                "-c",
                "user.name=tasu",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "local",
            ],
        )
        .unwrap();
        git(&data, &["remote", "add", "origin", &remote]).unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();

        assert_eq!(current_branch(&data).as_deref(), Some("main"));
    }

    #[test]
    fn an_unreachable_remote_does_not_rename_the_branch() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();
        git(&data, &["init", "-q", "-b", "master"]).unwrap();
        git(&data, &["add", "-A"]).unwrap();
        git(
            &data,
            &[
                "-c",
                "user.name=tasu",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "local",
            ],
        )
        .unwrap();

        let missing = dir.path().join("nope.git");
        ensure_repo(&data, Some(&missing.to_string_lossy())).unwrap();

        assert_eq!(current_branch(&data).as_deref(), Some("master"));
    }

    #[test]
    fn branch_reconciliation_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let remote = seeded_remote(dir.path(), "trunk");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();

        assert_eq!(current_branch(&data).as_deref(), Some("trunk"));
        assert!(
            is_settled(&data, &remote),
            "the remote should be remembered so later runs skip the ls-remote"
        );
        assert!(!is_settled(&data, "/some/other/remote.git"));
    }

    #[test]
    fn probe_remote_rejects_a_missing_repository() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.git");
        assert!(probe_remote(&missing.to_string_lossy()).is_err());
    }

    #[test]
    fn classifies_common_git_failures() {
        assert_eq!(
            Failure::classify("remote: Repository not found."),
            Failure::NotFound
        );
        assert_eq!(
            Failure::classify(
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
            ),
            Failure::Auth
        );
        assert_eq!(
            Failure::classify("fatal: unable to access 'x': Could not resolve host: github.com"),
            Failure::Network
        );
        assert_eq!(Failure::classify("something else"), Failure::Other);
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
    fn diverged_histories_merge_instead_of_conflicting() {
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

        // Publish a base board on `main` and point the bare HEAD at it, so a
        // clone checks it out.
        let seed = dir.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        write_board(&seed, &[("base", 5)]);
        git(&seed, &["init", "-q", "-b", "main"]).unwrap();
        git(&seed, &["add", "-A"]).unwrap();
        commit(&seed, "base");
        git(&seed, &["remote", "add", "origin", &remote]).unwrap();
        git(&seed, &["push", "-q", "origin", "main"]).unwrap();
        let _ = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .status();

        // A: a local commit that is never pushed.
        let a = dir.path().join("a");
        clone_repo(&remote, &a);
        add_task(&a, "from A", 6);
        commit(&a, "a");

        // B: a different commit, pushed.
        let b = dir.path().join("b");
        clone_repo(&remote, &b);
        add_task(&b, "from B", 7);
        commit(&b, "b");
        git(&b, &["push", "-q", "origin", "main"]).unwrap();

        // A pulls. The histories diverge; the boards must merge, not conflict.
        pull(&a, Some(&remote)).unwrap();
        let titles = board_titles(&a);
        for title in ["base", "from A", "from B"] {
            assert!(
                titles.iter().any(|seen| seen == title),
                "lost {title:?}: {titles:?}"
            );
        }

        // And it converges: A pushes, B pulls, both see everything.
        commit_push(&a, Some(&remote)).unwrap();
        pull(&b, Some(&remote)).unwrap();
        assert!(
            board_titles(&b).iter().any(|seen| seen == "from A"),
            "B did not get A"
        );
    }

    fn board_titles(repo: &Path) -> Vec<String> {
        crate::store::Store::new(repo.join("todos.json"))
            .load()
            .tasks()
            .iter()
            .map(|task| task.title.clone())
            .collect()
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
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();
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
