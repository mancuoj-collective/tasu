# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
  exit. Offline and unconfigured are both fine.
- `tasu add` for capturing without opening the app, plus `tasu config` and
  `tasu remote` (GitHub `owner/repo` shorthand, and a reachability check).

### Changed

- Replaced the flat, dated-todo model with the three-bucket pipeline. Tasks are
  no longer just a checklist; they sink as they age.

## [0.1.0] - 2026-09-22

Initial release: a flat terminal todo list with JSON persistence (add, edit,
toggle, delete) built with ratatui.

[Unreleased]: https://github.com/mancuoj-collective/tasu/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.2.0
[0.1.0]: https://github.com/mancuoj-collective/tasu/releases/tag/v0.1.0
