# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The board file now has a single atomic writer: the sync layer reads and writes
  `todos.json` through `Store`, so a sync write can no longer race a reload.
- A failure to adopt or create the sync repository is reported in the footer
  instead of being swallowed.
- Two tasks can no longer share a creation instant (the task identity), which
  would have made them silently collapse into one on a later merge.

## [0.13.0] - 2026-10-06

### Added

- README screenshots as vector SVG (`assets/kanban-{light,dark}.svg`), regenerated
  with `cargo run --example screenshot`, plus a short FAQ and bug/feature issue
  templates.

### Changed

- The footer and help label the bracket keys `[] move` instead of `[/] bucket`.
- The README now states that `tasu update` cannot replace a running `.exe` on
  Windows and prints the command instead.

## [0.12.0] - 2026-10-06

### Fixed

- A board can no longer hold two tasks with the same creation time: duplicates
  older syncs could leave are collapsed, on load and on merge, to the
  further-along copy.

### Removed

- All backwards-compatibility scaffolding, since only the author uses tasu: the
  config file's old-location fallback, the `config.json` untracking and ignore
  entry, and the on-disk schema version with its checks.

## [0.11.0] - 2026-10-06

### Changed

- Board merges (first connect and divergence) identify tasks by their creation
  time instead of title + creation time, so a renamed task merges into the same
  task rather than forking into a duplicate.

## [0.10.0] - 2026-10-06

### Fixed

- Sync no longer rebases the board, so two machines that both changed
  `todos.json` can no longer strand the repository in a rebase conflict with the
  push rejected. A pull now fetches, and when the histories diverge it resets to
  the remote and merges the boards task by task (a completed or dropped copy
  wins over an open one).

## [0.9.0] - 2026-10-06

### Fixed

- The config file no longer lives inside the synced data repository. On macOS
  and Windows the platform config dir equals the data dir, so `config.json` sat
  in the Git working tree; a checkout of a repository that tracked it could
  overwrite the file and silently clear the remote. It now lives at
  `$XDG_CONFIG_HOME/tasu/config.json`, and any committed `config.json` is
  untracked on the next run.

### Changed

- Starting tasu no longer runs a `git ls-remote` to reconcile the branch on
  every launch; that result is remembered per remote, so opening only does the
  one pull.

## [0.8.0] - 2026-10-06

### Changed

- The help overlay no longer shows the sync error. A failure is summarised in
  the footer as `✗ <reason> · run tasu sync`, with the full git error left to
  `tasu sync`.
- Quitting returns to the shell immediately: the board is already saved, and the
  final commit+push runs in a detached process instead of blocking on the
  network.

## [0.7.0] - 2026-10-06

### Added

- A fuller command line: `tasu list` (alias `ls`, optional bucket, `--json`),
  `tasu history [done|dropped]` (both views by default, `--json`),
  `tasu done|drop|move` by exact title, `tasu config --json`, and
  `tasu completions <shell>`. The everyday commands carry short aliases (`a`,
  `ls`, `h`, `d`, `x`, `mv`), listed in `tasu --help`.
- `tasu add` now confirms what it captured.
- `tasu update`, which upgrades tasu using however it was installed (Homebrew,
  the installer script, or Cargo), instead of guessing.

## [0.6.0] - 2026-10-05

### Changed

- Connecting a remote from a machine that already has a board no longer
  overwrites the remote's tasks: the two boards are merged (union, exact
  duplicates dropped), so neither machine loses anything.
- The help overlay is wider with roomier side padding. Long paths and URLs wrap
  long-top/short-bottom and right-align with the key column, and every binding
  is listed (the arrow/enter/home/end aliases and `?`); the lone history-tab
  note is gone, since the history keys are shown in its own footer.

### Fixed

- The help overlay shows a failed sync as a one-line reason (e.g. `✗
  authentication failed`) instead of a raw git dump; the full error stays in
  `tasu sync`.
- The footer shows `✗ sync failed · <reason>` instead of a bare dot.
- `tasu remote` prints a single, prioritised failure line (cause + one fix)
  instead of repeating the URL and dumping a generic multi-line hint.

## [0.5.0] - 2026-10-05

### Fixed

- Sync no longer creates a machine-dependent branch (e.g. `master` on Windows):
  tasu adopts the remote's default branch, or uses `main`, on every machine.
- `tasu remote <url>` now probes the remote **before** saving it. An
  unreachable, missing or unauthorized repository is not persisted, and any
  previously set remote is cleared instead of being retried forever.
- `pull` names the branch explicitly, so a freshly adopted repository without an
  upstream no longer fails with "no tracking information".

## [0.4.0] - 2026-10-05

### Added

- Prebuilt binaries for macOS (arm64/x86_64), Linux (arm64/x86_64) and Windows
  (x86_64), plus `curl | sh` and PowerShell installers.
- A Homebrew tap: `brew install mancuoj/tap/tasu` installs a prebuilt binary, no
  Rust toolchain required.

### Changed

- Releases are built by [dist](https://opensource.axo.dev/cargo-dist) and
  published to crates.io from a separate workflow.

## [0.3.0] - 2026-10-05

Hardening on top of the 0.2.0 rewrite: no silent failures, and sync that
recovers instead of getting stuck.

### Fixed

- Every previously-swallowed error is now visible: sync failures (footer dot
  and the reason in `?` help), a failed final push on exit, a board that could
  not be written, a malformed config file, and a failed corrupt-file backup.
- Changing the remote now actually updates `origin`.
- A rejected push retries with a pull first, so a non-fast-forward recovers.
- A conflicting pull aborts the rebase instead of leaving the repository stuck.
- No remote means no git at all; the config file is never committed.

### Added

- `tasu sync` to run one pull-then-push and print the real error.
- `tasu remote` accepts a GitHub `owner/repo` shorthand and checks reachability.
- A last-sync-error line in the help overlay.

## [0.2.0] - 2026-10-05

A rewrite: the product is now a list that ages itself, with a layered
architecture and optional git sync.

### Added

- Three aging buckets — **today**, **this week**, **later** — that demote on a
  daily and weekly cadence, settled lazily from the local date.
- A no-border TUI with vertical sections on narrow terminals and three columns
  at 100+ columns.
- A history overlay (`c`) with **done** and **dropped** tabs; either can be
  restored to its original bucket.
- Title search in the history overlay, and a drop/archive action that never
  deletes.
- Optional git sync against any remote (private repo, Gist, self-hosted, or a
  local path): pull on start, debounced push after changes, best-effort push on
  exit.
- `tasu add` for capturing without opening the app, plus `tasu config` and
  `tasu remote`.

### Changed

- Replaced the flat, dated-todo model with the three-bucket pipeline. Tasks are
  no longer just a checklist; they sink as they age.

## [0.1.0] - 2026-09-22

Initial release: a flat terminal todo list with JSON persistence (add, edit,
toggle, delete) built with ratatui.

[Unreleased]: https://github.com/mancuoj-collective/tasu/compare/v0.13.0...HEAD
[0.13.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.13.0
[0.12.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.12.0
[0.11.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.11.0
[0.10.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.10.0
[0.9.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.9.0
[0.8.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.8.0
[0.7.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.7.0
[0.6.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.6.0
[0.5.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.5.0
[0.4.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.4.0
[0.3.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.3.0
[0.2.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.2.0
[0.1.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.1.0
