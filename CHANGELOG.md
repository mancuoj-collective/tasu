# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/mancuoj-collective/tasu/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.7.0
[0.6.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.6.0
[0.5.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.5.0
[0.4.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.4.0
[0.3.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.3.0
[0.2.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.2.0
[0.1.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.1.0
