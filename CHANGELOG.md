# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-10-05

A rewrite: the product is now a list that ages itself, with a layered
architecture and optional git sync. (0.2.0 was tagged but never published, so
0.3.0 is the first release of the rewrite.)

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
  exit. Offline and unconfigured are both fine.
- `tasu add` for capturing without opening the app, plus `tasu config`,
  `tasu remote` (GitHub `owner/repo` shorthand, and a reachability check) and
  `tasu sync` for running one round and reporting the real error.

### Changed

- Replaced the flat, dated-todo model with the three-bucket pipeline. Tasks are
  no longer just a checklist; they sink as they age.

### Fixed

- Every previously-swallowed error is now visible: sync failures (footer dot
  and the reason in `?` help), a failed final push on exit, a board that could
  not be written, a malformed config file, and a failed corrupt-file backup.
- Changing the remote now actually updates `origin`.
- A rejected push retries with a pull first, so a non-fast-forward recovers.
- A conflicting pull aborts the rebase instead of leaving the repository stuck.
- No remote means no git at all; the config file is never committed.

## [0.1.0] - 2026-09-22

Initial release: a flat terminal todo list with JSON persistence (add, edit,
toggle, delete) built with ratatui.

[Unreleased]: https://github.com/mancuoj-collective/tasu/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.3.0
[0.1.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.1.0
