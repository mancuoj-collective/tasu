//! The local repository: adopting, cloning or grafting it onto a remote, the
//! branch every machine agrees on, and the `.gitignore` that keeps transient
//! files out of the synced tree. Every step returns its failure, so a broken
//! setup reaches the footer instead of looking healthy.

use std::path::Path;

use crate::store::{MergeSide, Store, merge_files};

use super::git;
use super::{Error, NETWORK_TIMEOUT};

/// Branch used when the remote has no branch to adopt. A fixed name keeps every
/// machine on the same branch: the host's `init.defaultBranch` (often `master`
/// on Windows) must never leak into the data repository.
pub(super) const DEFAULT_BRANCH: &str = "main";

/// The board file inside the sync working tree. Reads and writes go through
/// `Store`, so `todos.json` has exactly one (atomic) write path shared with the
/// app.
pub(super) fn board_store(repo: &Path) -> Store {
    Store::new(repo.join("todos.json"))
}

/// Make the data directory a usable repository for `remote`, or do nothing when
/// there is no remote. Adopts an existing repository, clones into an empty
/// directory (a second machine), or grafts local data onto a remote that
/// already has history.
pub(super) fn ensure_repo(repo: &Path, remote: Option<&str>) -> Result<(), Error> {
    if repo.join(".git").exists() {
        return reconcile(repo, remote);
    }

    // Without a remote tasu is purely local and never shells out to git.
    let Some(url) = remote else {
        return Ok(());
    };

    std::fs::create_dir_all(repo).map_err(|err| Error::io(err.to_string()))?;
    if is_empty_dir(repo) {
        clone(url, repo)?;
        return adopt_clone(repo, url);
    }
    graft(repo, url)
}

/// An existing repository: point `origin` at the configured URL and, once per
/// remote, reconcile the branch name with the remote's default branch.
fn reconcile(repo: &Path, remote: Option<&str>) -> Result<(), Error> {
    ensure_gitignore(repo)?;
    let Some(url) = remote else {
        return Ok(());
    };
    set_origin(repo, url)?;
    // The `ls-remote` this needs is a network round-trip we do not want on every
    // launch. An unreachable remote is not marked, so it is retried next time.
    if !is_settled(repo, url)
        && let Ok(branch) = remote_default_branch_result(url)
    {
        if let Some(branch) = branch {
            name_branch(repo, &branch)?;
        }
        mark_settled(repo, url)?;
    }
    Ok(())
}

/// A fresh clone: check the checkout out first, because an untracked
/// `.gitignore` written before the checkout would block git from creating the
/// tracked one, leaving the board unpopulated.
fn adopt_clone(repo: &Path, url: &str) -> Result<(), Error> {
    settle_head(repo, &target_branch(url))?;
    ensure_gitignore(repo)?;
    mark_settled(repo, url)
}

/// Existing local data meets a remote that may already have history. The
/// remote's history is adopted, the boards are merged, then the result is
/// committed on top and pushed. No unrelated-history merge, which proved
/// unreliable across git builds.
fn graft(repo: &Path, url: &str) -> Result<(), Error> {
    // Name the branch before the first commit: `git init` alone would use the
    // host's default (e.g. `master`), splitting the remote into two branches.
    let branch = target_branch(url);
    git::run(repo, &["init", "-q"])?;
    name_branch(repo, &branch)?;
    set_origin(repo, url)?;

    // Fetch the remote's current tip into a private ref, independent of
    // remote-tracking branches (which are not always created).
    let fetched = git::run(
        repo,
        &[
            "fetch",
            "--quiet",
            "origin",
            &format!("+refs/heads/{branch}:refs/tasu/remote"),
        ],
    )
    .is_ok()
        || git::run(
            repo,
            &["fetch", "--quiet", "origin", "+HEAD:refs/tasu/remote"],
        )
        .is_ok();

    // Read both boards and merge them **before** the working tree is touched.
    // If a board cannot be read, the merge errors and the local files —
    // `todos.json` included — stay exactly as they were, because nothing has
    // been reset yet.
    let remote_board = if fetched {
        git::bytes(repo, &["show", "refs/tasu/remote:todos.json"])
    } else {
        None
    };
    let local_board = board_store(repo).read_bytes();
    let board = match (local_board, remote_board) {
        (Some(local), Some(remote)) => Some(
            merge_files(&local, &remote)
                .map_err(|side| Error::io(unreadable_board(side, "local", "remote")))?,
        ),
        (board, None) => board,
        (None, board) => board,
    };

    // The merge succeeded, so it is safe to adopt the remote's tree. The merged
    // board is written back on top of it, so the merged result — not the remote's
    // board — is what gets committed.
    if fetched {
        git::run(repo, &["reset", "--hard", "refs/tasu/remote"])?;
    }
    if let Some(bytes) = board.filter(|bytes| !bytes.is_empty()) {
        board_store(repo)
            .write_bytes(&bytes)
            .map_err(|err| Error::io(err.to_string()))?;
    }
    commit_all(repo, "tasu: local")?;
    mark_settled(repo, url)
}

fn is_empty_dir(repo: &Path) -> bool {
    std::fs::read_dir(repo)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn clone(url: &str, repo: &Path) -> Result<(), Error> {
    let mut cmd = git::program();
    cmd.args(["clone", "--quiet", url]).arg(repo);
    git::bounded(&mut cmd, NETWORK_TIMEOUT).map(|_| ())
}

/// Stage everything tasu owns and commit it, unless nothing is staged. Decided
/// with a machine-readable check: `git commit`'s own "nothing to commit" message
/// is locale-dependent.
pub(super) fn commit_all(repo: &Path, message: &str) -> Result<(), Error> {
    ensure_gitignore(repo)?;
    git::run(repo, &["add", "-A"])?;
    if git::ok(repo, &["diff", "--cached", "--quiet"]) {
        return Ok(());
    }
    git::run(
        repo,
        &[
            "-c",
            "user.name=tasu",
            "-c",
            "user.email=tasu@localhost",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )
}

/// Point `origin` at `url`. `set-url` works on an existing origin, unlike
/// `remote add`, which fails and leaves a stale URL; `add` covers a missing one.
fn set_origin(repo: &Path, url: &str) -> Result<(), Error> {
    git::run(repo, &["remote", "set-url", "origin", url])
        .or_else(|_| git::run(repo, &["remote", "add", "origin", url]))
}

/// The branch tasu syncs on: the remote's default branch when it already has
/// one, otherwise [`DEFAULT_BRANCH`].
fn target_branch(remote: &str) -> String {
    remote_default_branch_result(remote)
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_BRANCH.to_string())
}

/// The remote's default branch, read from its symbolic `HEAD`. `Err` when the
/// remote is unreachable; `Ok(None)` when it is readable but has no default
/// branch.
fn remote_default_branch_result(remote: &str) -> Result<Option<String>, Error> {
    let mut cmd = git::program();
    cmd.args(["ls-remote", "--symref", remote, "HEAD"]);
    let output = git::bounded(&mut cmd, NETWORK_TIMEOUT)?;
    // "ref: refs/heads/main\tHEAD"
    Ok(output.lines().find_map(|line| {
        let name = line
            .strip_prefix("ref:")?
            .trim()
            .strip_prefix("refs/heads/")?;
        name.split_whitespace().next().map(str::to_string)
    }))
}

/// Remember the remote whose default branch we already reconciled. Stored in the
/// repo's own config, never committed.
fn mark_settled(repo: &Path, url: &str) -> Result<(), Error> {
    git::run(repo, &["config", "tasu.remote", url])
}

/// Whether the branch for `url` was already reconciled on a previous run.
pub(super) fn is_settled(repo: &Path, url: &str) -> bool {
    git::text(repo, &["config", "--get", "tasu.remote"]).as_deref() == Some(url)
}

/// The current branch, or `None` on a detached HEAD or an error.
pub(super) fn current_branch(repo: &Path) -> Option<String> {
    let output = git::in_repo(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Make sure a freshly cloned repository is on a real branch: normalise the
/// name when HEAD is born, otherwise check out the remote branch we expected, or
/// any branch the remote does have.
fn settle_head(repo: &Path, target: &str) -> Result<(), Error> {
    if git::ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return name_branch(repo, target);
    }
    if git::ok(
        repo,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{target}"),
        ],
    ) {
        git::run(repo, &["checkout", "--quiet", target])
    } else if let Some(branch) = first_remote_branch(repo) {
        git::run(repo, &["checkout", "--quiet", &branch])
    } else {
        name_branch(repo, target)
    }
}

/// The first branch under `origin/`, for a remote whose symbolic `HEAD` is
/// stale. `None` when the remote has no branches.
fn first_remote_branch(repo: &Path) -> Option<String> {
    let output = git::in_repo(
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
pub(super) fn name_branch(repo: &Path, target: &str) -> Result<(), Error> {
    let Some(current) = current_branch(repo) else {
        return point_head(repo, target);
    };
    if current == target {
        return Ok(());
    }
    if !git::ok(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return point_head(repo, target);
    }
    // A born branch: `branch -m` moves the ref and HEAD together. If the target
    // already exists locally we cannot rename, so switch to it instead.
    git::run(repo, &["branch", "-m", target])
        .or_else(|_| git::run(repo, &["checkout", "--quiet", target]))
}

/// Name the side of a failed board merge, so a sync result says which file could
/// not be read instead of silently dropping its tasks.
fn unreadable_board(side: MergeSide, base: &str, other: &str) -> String {
    let name = match side {
        MergeSide::Base => base,
        MergeSide::Other => other,
    };
    format!("{name} board could not be read; sync stopped to avoid dropping tasks")
}

fn point_head(repo: &Path, target: &str) -> Result<(), Error> {
    git::run(
        repo,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{target}")],
    )
}

/// Keep the transient files `Store` writes — a half-written temp, a corrupt
/// backup — out of the synced repository. Added idempotently.
pub(super) fn ensure_gitignore(repo: &Path) -> Result<(), Error> {
    let path = repo.join(".gitignore");
    let mut text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(Error::io(format!(
                "could not read {}: {err}",
                path.display()
            )));
        }
    };
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
    if changed {
        std::fs::write(&path, &text)
            .map_err(|err| Error::io(format!("could not write {}: {err}", path.display())))?;
    }
    Ok(())
}
