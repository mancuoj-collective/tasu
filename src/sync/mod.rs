mod failure;
mod git;
mod repo;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::domain::Board;
use crate::store::Damage;

pub use failure::{Error, Failure};
use repo::{DEFAULT_BRANCH, board_store, commit_all, current_branch, ensure_repo};

/// Network operations are bounded so a hung connection cannot freeze a caller.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
const PUSH_TIMEOUT: Duration = Duration::from_secs(8);

/// Git-backed sync for a single writer.
///
/// The data directory *is* the repository working tree. All git work happens on
/// a worker thread; the app talks to it through a channel and is never blocked
/// by the network. Without a remote, nothing here ever shells out to git.
pub struct Sync {
    jobs: Sender<Job>,
    results: Receiver<JobOutcome>,
    /// One-shot outcome of adopting/creating the repository, sent before any
    /// job runs so a broken setup is visible instead of looking idle.
    init: Receiver<Result<(), Error>>,
}

enum Job {
    Pull,
    CommitPush,
}

/// What a finished pull produced, handed back to the app.
pub enum Pulled {
    /// Level with or ahead of the remote: nothing to merge.
    UpToDate,
    /// The remote is ahead or the histories diverged. The local branch has been
    /// moved to the remote tip so the next push fast-forwards, and the remote's
    /// board (when the remote has one) is returned for the caller to merge.
    Merge(Option<Board>),
}

/// The outcome of one worker job.
pub enum JobOutcome {
    Pulled(Pulled),
    Pushed,
    Failed(Error),
}

impl Sync {
    pub fn new(repo: PathBuf, remote: Option<String>) -> Self {
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (results_tx, results_rx) = mpsc::channel::<JobOutcome>();
        let (init_tx, init_rx) = mpsc::channel::<Result<(), Error>>();

        // The worker owns the repo and remote; the app only talks to it over
        // the channels.
        thread::spawn(move || {
            // Adopt or create the repository once, before the first job, and
            // report the outcome so a failure is not silently swallowed.
            if init_tx.send(ensure_repo(&repo, remote.as_deref())).is_err() {
                return;
            }
            while let Ok(job) = jobs_rx.recv() {
                let outcome = match job {
                    Job::Pull => match pull(&repo, remote.as_deref()) {
                        Ok(pulled) => JobOutcome::Pulled(pulled),
                        Err(err) => JobOutcome::Failed(err),
                    },
                    Job::CommitPush => match commit_push(&repo, remote.as_deref()) {
                        Ok(()) => JobOutcome::Pushed,
                        Err(err) => JobOutcome::Failed(err),
                    },
                };
                if results_tx.send(outcome).is_err() {
                    break;
                }
            }
        });

        Self {
            jobs: jobs_tx,
            results: results_rx,
            init: init_rx,
        }
    }

    /// Fetch the latest remote state. The remote's board comes back through
    /// [`Sync::poll`] for the app to merge, so the worker never writes
    /// `todos.json` and cannot race the app's own save.
    pub fn pull(&self) -> Result<(), Error> {
        self.jobs
            .send(Job::Pull)
            .map_err(|_| Error::io("sync worker stopped"))
    }

    /// Stage, commit and push. Debounced by the caller.
    pub fn commit_push(&self) -> Result<(), Error> {
        self.jobs
            .send(Job::CommitPush)
            .map_err(|_| Error::io("sync worker stopped"))
    }

    /// The next finished job, `Ok(None)` when the worker has nothing yet. A
    /// disconnected channel means the worker thread is gone, which is reported
    /// as an error rather than looking like an idle worker forever.
    pub fn poll(&self) -> Result<Option<JobOutcome>, Error> {
        match self.results.try_recv() {
            Ok(outcome) => Ok(Some(outcome)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::io("sync worker stopped")),
        }
    }

    /// The one-shot repository-setup result, once the worker has produced it.
    /// `Ok(None)` until then, and after the first call. A disconnected channel
    /// means the worker died before it could report, which is surfaced the same
    /// way.
    pub fn poll_init(&self) -> Result<Option<Result<(), Error>>, Error> {
        match self.init.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::io("sync worker stopped")),
        }
    }
}

/// One synchronous commit+push, for one-shot commands and process exit.
pub fn commit_now(repo: &Path, remote: Option<&str>) -> Result<(), Error> {
    ensure_repo(repo, remote)?;
    commit_push(repo, remote)
}

/// One synchronous pull, used before a one-shot command reads the board so a
/// CLI-only machine does not diverge from the remote. Returns any [`Damage`]
/// from the local board that could not be read, so the caller can surface it
/// instead of letting the tasks look lost.
pub fn pull_now(repo: &Path, remote: Option<&str>) -> Result<Option<Damage>, Error> {
    ensure_repo(repo, remote)?;
    apply_pull(repo, pull(repo, remote)?)
}

/// Merge a pull result into the on-disk board. The one-shot CLI is a single
/// process, so it writes the file directly; the TUI instead hands the board to
/// its model, which stays the file's only writer. The `Damage`, when the local
/// board could not be read (and was moved aside by `Store::load`), is returned
/// rather than dropped, so a pull can never quietly replace a board it could not
/// parse.
fn apply_pull(repo: &Path, pulled: Pulled) -> Result<Option<Damage>, Error> {
    let Pulled::Merge(Some(remote)) = pulled else {
        return Ok(None);
    };
    let store = board_store(repo);
    // Remote is the shared base; the local board fills in and advances.
    let loaded = store.load();
    let merged = remote.merged_with(&loaded.board);
    store
        .save(&merged)
        .map_err(|err| Error::io(err.to_string()))?;
    Ok(loaded.damaged)
}

/// Check that a remote is reachable and that credentials work, without asking
/// for input. Empty repositories count as reachable.
pub fn probe_remote(url: &str) -> Result<(), Error> {
    let mut cmd = git::program();
    cmd.args(["ls-remote", url]);
    git::bounded(&mut cmd, NETWORK_TIMEOUT).map(|_| ())
}

/// Bring the local branch up to date with the remote without ever producing a
/// rebase conflict. The board is the whole state, so history carries no extra
/// meaning: the remote's board is handed back to be merged **task by task**, and
/// the branch is moved with `--soft`, leaving the working tree — the app's
/// `todos.json` in particular — exactly as the app has it.
fn pull(repo: &Path, remote: Option<&str>) -> Result<Pulled, Error> {
    if remote.is_none() {
        return Ok(Pulled::UpToDate);
    }
    // Nothing to pull until the first commit exists (fresh remote).
    if !git::ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return Ok(Pulled::UpToDate);
    }
    let Some(branch) = current_branch(repo) else {
        return Ok(Pulled::UpToDate);
    };

    // Fetch the remote branch into a private ref (remote-tracking refs are not
    // always created), then compare histories.
    let mut fetch = git::in_repo(
        repo,
        &[
            "fetch",
            "--quiet",
            "origin",
            &format!("+refs/heads/{branch}:refs/tasu/remote"),
        ],
    );
    git::bounded(&mut fetch, NETWORK_TIMEOUT)?;

    // If the remote is already reachable from HEAD, we are level or ahead.
    if git::ok(
        repo,
        &["merge-base", "--is-ancestor", "refs/tasu/remote", "HEAD"],
    ) {
        return Ok(Pulled::UpToDate);
    }

    // The remote is ahead, or the two histories diverged. Read its board out of
    // the commit instead of checking the commit out, so `todos.json` is never
    // written behind the app's back; the caller merges it in and saves it, and
    // is therefore the file's only writer. A board that cannot be read is an
    // error, never a silent empty list.
    let remote_board = match git::bytes(repo, &["show", "refs/tasu/remote:todos.json"]) {
        Some(bytes) => Some(parse_board(&bytes)?),
        None => None,
    };
    // `--soft` moves the branch (and HEAD) without touching the index or the
    // working tree, so the next commit fast-forwards from the remote tip while
    // the app's file is preserved for the caller to merge.
    git::run(repo, &["reset", "--soft", "refs/tasu/remote"])?;
    Ok(Pulled::Merge(remote_board))
}

/// Parse a board read out of the repository. A board that cannot be parsed is
/// an error, never a silent empty list: that would drop a machine's tasks.
fn parse_board(bytes: &[u8]) -> Result<Board, Error> {
    crate::store::board_from_bytes(bytes).ok_or_else(|| {
        Error::io("the remote board could not be read; sync stopped to avoid dropping tasks")
    })
}

fn commit_push(repo: &Path, remote: Option<&str>) -> Result<(), Error> {
    commit_push_with(repo, remote, PUSH_TIMEOUT)
}

/// `commit_push` with an explicit push deadline, so tests can allow for a slow
/// machine without loosening the bound the app ships with.
fn commit_push_with(repo: &Path, remote: Option<&str>, timeout: Duration) -> Result<(), Error> {
    if remote.is_none() {
        return Ok(());
    }
    commit_all(repo, "tasu: sync")?;
    // Push to the explicit branch name instead of a bare `HEAD`, so a stray
    // local branch can never create a second branch on the remote.
    let branch = current_branch(repo).unwrap_or_else(|| DEFAULT_BRANCH.to_string());
    let mut cmd = git::in_repo(
        repo,
        &[
            "push",
            "--quiet",
            "-u",
            "origin",
            &format!("HEAD:refs/heads/{branch}"),
        ],
    );
    git::bounded(&mut cmd, timeout).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::git::run as git;
    use super::repo::is_settled;
    use super::*;

    /// Shadow the production `commit_push` with a generous deadline: CI —
    /// Windows especially — can be slow when these tests spawn many git
    /// processes in parallel.
    fn commit_push(repo: &Path, remote: Option<&str>) -> Result<(), Error> {
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
    fn pushing_with_nothing_to_commit_is_a_noop() {
        // Regression: `git commit` fails with "nothing to commit" when nothing
        // is staged, and that message is locale-dependent. The commit path must
        // decide with a machine-readable check, so this is Ok, not Err.
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("todos.json"), "{\"tasks\":[]}").unwrap();

        ensure_repo(&data, Some(&remote)).unwrap();
        commit_push(&data, Some(&remote)).unwrap();
        // Nothing changed since the first push.
        commit_push(&data, Some(&remote)).unwrap();
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
        let mut board = store.load().board;
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
        pull_applied(&a, &remote);
        let titles = board_titles(&a);
        for title in ["base", "from A", "from B"] {
            assert!(
                titles.iter().any(|seen| seen == title),
                "lost {title:?}: {titles:?}"
            );
        }

        // And it converges: A pushes, B pulls, both see everything.
        commit_push(&a, Some(&remote)).unwrap();
        pull_applied(&b, &remote);
        assert!(
            board_titles(&b).iter().any(|seen| seen == "from A"),
            "B did not get A"
        );
    }

    /// Pull and apply, the way the one-shot CLI does. The TUI instead hands the
    /// returned board to its model, which is the file's only writer.
    fn pull_applied(repo: &Path, remote: &str) {
        let _ = apply_pull(repo, pull(repo, Some(remote)).unwrap()).unwrap();
    }

    #[test]
    fn pull_reads_the_remote_board_without_touching_the_local_file() {
        // Regression: the worker used to reset `todos.json` to the remote's
        // version itself, which could clobber a save the app made while the pull
        // was in flight. A pull must leave the local file alone and hand the
        // remote board back for the caller to merge.
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

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

        // A local machine with its own unpushed task.
        let a = dir.path().join("a");
        clone_repo(&remote, &a);
        add_task(&a, "local only", 6);
        commit(&a, "a");

        // The remote moves ahead with a different task.
        let b = dir.path().join("b");
        clone_repo(&remote, &b);
        add_task(&b, "from B", 7);
        commit(&b, "b");
        git(&b, &["push", "-q", "origin", "main"]).unwrap();

        let pulled = pull(&a, Some(&remote)).unwrap();
        let Pulled::Merge(Some(remote_board)) = pulled else {
            panic!("expected the remote board to be handed back");
        };

        // The local file is untouched: the app's own task is still there and the
        // remote's task has not been written behind its back.
        let local = board_titles(&a);
        assert!(local.iter().any(|t| t == "local only"), "{local:?}");
        assert!(!local.iter().any(|t| t == "from B"), "{local:?}");

        // The returned board carries the remote side, and merging yields the
        // union without either task being lost.
        let titles: Vec<_> = remote_board
            .tasks()
            .iter()
            .map(|task| task.title.as_str())
            .collect();
        assert!(titles.contains(&"from B"), "{titles:?}");
    }

    #[test]
    fn pull_reports_a_corrupt_local_board() {
        // Regression: `apply_pull` used to keep only `.board` from `Store::load`,
        // so a local `todos.json` that could not be read was replaced by the
        // remote's copy with no warning — the tasks looked lost.
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

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

        // A clone whose working-tree board cannot be read.
        let a = dir.path().join("a");
        clone_repo(&remote, &a);
        std::fs::write(a.join("todos.json"), "{ not a board").unwrap();

        // The remote moves ahead, so the pull takes the merge path.
        add_task(&seed, "from seed", 6);
        commit(&seed, "ahead");
        git(&seed, &["push", "-q", "origin", "main"]).unwrap();

        let damage = pull_now(&a, Some(&remote))
            .expect("the pull itself succeeds")
            .expect("the unreadable local board must be reported, not swallowed");
        assert!(damage.backup.is_some(), "the corrupt board was moved aside");
    }

    fn board_titles(repo: &Path) -> Vec<String> {
        crate::store::Store::new(repo.join("todos.json"))
            .load()
            .board
            .tasks()
            .iter()
            .map(|task| task.title.clone())
            .collect()
    }

    #[test]
    fn a_failed_graft_merge_leaves_the_local_board_in_place() {
        // Regression: the graft used to `reset --hard` to the remote's tree
        // *before* merging, so a board that could not be read took the local
        // board down with it. The merge must happen first, while the local files
        // are still untouched.
        let dir = tempfile::tempdir().unwrap();
        let remote = bare_remote(dir.path());

        // The remote carries a `todos.json` that is not a board at all.
        let seed = dir.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        std::fs::write(seed.join("todos.json"), "{ not a board").unwrap();
        git(&seed, &["init", "-q", "-b", "main"]).unwrap();
        commit(&seed, "seed");
        git(&seed, &["remote", "add", "origin", &remote]).unwrap();
        git(&seed, &["push", "-q", "origin", "main"]).unwrap();
        let _ = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .status();

        // A local machine with a real board that must survive the failure.
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        write_board(&data, &[("keep me", 5)]);

        let err = ensure_repo(&data, Some(&remote)).unwrap_err();
        assert!(err.detail().contains("could not be read"), "{err}");

        let titles = board_titles(&data);
        assert!(
            titles.iter().any(|title| title == "keep me"),
            "the local board was lost: {titles:?}"
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
