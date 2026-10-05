# tasu — architecture

This document is derived from `PRODUCT.md`. The product is the only source of
constraints: every module here must be able to answer "which part of the product
does this serve?".

## Principles

1. **Pure domain, injected clock.** The automatic `today / week / later`
   demotion is the heart of the product, so it must be testable with fixed
   timestamps — no terminal, no system clock.
2. **The UI is a function of state.** The same `Model` renders as a narrow
   three-section list or a wide three-column board; rendering only reads.
3. **I/O stays out of the loop.** Git is a network operation and the data file
   can change underneath us; neither may block a keypress.
4. **Two front-ends, one core.** The TUI and `tasu add` share `domain` +
   `store` + `sync`.
5. **Minimal dependencies.** `clap` and `chrono` are the only real additions;
   git is a system command and file changes are detected by polling mtime.

## Layering

```
main.rs ──▶ cli.rs (no subcommand → TUI, otherwise a one-shot command)
    │
    ├── TUI   app/ + ui/          Event → Action → update → Effect
    └── CLI   command/            one-shot: write + best-effort push
    │
    ▼
domain/                            pure logic, zero I/O, `now` injected
    Board · Task · settle()
    │
    ├── store/                     atomic JSON persistence
    └── sync/                      git shell + worker thread
```

```
src/
  main.rs            entry: wires store/sync, runs the loop and its effects
  cli.rs             clap definition
  config.rs          settings: data dir / remote (env > config file > default)
  command/mod.rs     one-shot commands: add, config, remote, sync
  domain/
    task.rs          Task / TaskState / Bucket
    board.rs         Board: collection + operations + per-bucket queries
    settle.rs        pure settle(&mut Board, now)
  store/mod.rs       load / atomic save / mtime / schema version / merge_files
  sync/mod.rs        git: probe, pull, commit+push, branch resolution, error
                     classification; worker thread + result channel
  app/
    model.rs         Model = Board + UiState
    action.rs        Action / Effect enums
    update.rs        update(&mut Model, Action) -> Vec<Effect>
  ui/
    mod.rs           width-based layout selection + footer + modal dispatch
    theme.rs         cold palette
    sections.rs      narrow vertical stack
    kanban.rs        wide three columns
    modal.rs         add/edit, history, help
    components.rs    task rows, headers (ISO week), toast
```

## Data contract (on disk)

A single `todos.json` in the data directory (the platform data dir by default,
overridable with `TASU_DATA`). The schema is **versioned** to leave room for
migration.

```json
{
  "version": 1,
  "tasks": [
    {
      "title": "Nand2Tetris chapter 6",
      "state": "open",              // open | done | archived
      "bucket": "today",            // today | week | later
      "bucket_since": "2026-10-04T09:12:00.123+08:00",
      "created_at":   "2026-10-04T09:12:00.123+08:00",
      "completed_at": null,
      "archived_at":  null
    }
  ]
}
```

Key decisions:

- **No id.** Single writer, no concurrent merge, and operations address tasks by
  position. Bucket order is derived from `created_at` (with nanoseconds), so
  **restoring a task means moving it back to its bucket and resetting
  `bucket_since`** — ordering falls back into place with no extra bookkeeping.
- **`bucket` survives `done` / `archived`**, so undo and restore have an
  "original bucket" to return to.
- All times are `chrono::DateTime<Local>`.
- `store::merge_files(local, remote)` unions two serialized boards,
  de-duplicating identical tasks, and refuses (returns `None`) if either side is
  unreadable or from another schema version.

## Domain and the state machine

```rust
// domain/task.rs
enum TaskState { Open, Done, Archived }
enum Bucket    { Today, Week, Later }

struct Task {
    title: String,
    state: TaskState,
    bucket: Bucket,
    bucket_since: DateTime<Local>,  // when it entered the current bucket
    created_at: DateTime<Local>,
    completed_at: Option<DateTime<Local>>,
    archived_at: Option<DateTime<Local>>,
}
```

```rust
// domain/settle.rs — pure, clock injected
fn settle(board: &mut Board, now: DateTime<Local>) -> bool {
    // Today on an earlier day          → Week
    // Week in an earlier ISO week(year)→ Later
    // (only Open tasks age; automatic demotion leaves bucket_since alone)
}
```

- `iso_week` compares `(ISO year, week)`, so weeks across a year boundary order
  correctly (week 1 can belong to the previous year).
- **Only `Open` tasks age**; `Done` and `Archived` are frozen.
- Automatic demotion does not reset `bucket_since`, so a task left stale for
  longer than a week cascades straight from `Today` to `Later` in one pass.
- `settle` is idempotent for a fixed `now` and returns whether anything moved;
  it runs after load and on every tick, and a change triggers a save.

`Board` operations (all take `now`): `add`, `complete`, `archive`, `restore`
(done → open, `bucket_since = now`), `unarchive`, `move_bucket(±1)`, `pin_today`,
`rename`, `merged_with`. Queries: `open_in(bucket)` (newest first), `done()`,
`archived()`.

## Effects

`update` never touches I/O; it returns one of:

| Effect | Meaning |
| --- | --- |
| `Save` | Atomically write `todos.json`, mark sync dirty, refresh `last_mtime` |
| `Quit` | Stop the loop; save a final time and flush one last push |

The startup pull is not triggered by `update`: the runtime sends one background
pull, and the file it changes is picked up by **mtime detection** as
`Action::Reload`. The runtime tracks `last_mtime` and refreshes it right after
its own writes, which is what separates "our write" from "an external one".

## Main loop and concurrency

- The loop: draw → drain sync results and check mtime → wait up to `TICK`
  (200 ms) for a key → `update` → apply effects.
- **`Runtime`** (in `main.rs`) owns the `Store`, the optional `Sync` worker and
  the debounce state (`dirty`, `last_change`, `retry_at`, `in_flight`,
  `needs_pull`).
- **Debounced push**: a change sets `dirty`; once nothing is in flight and
  `DEBOUNCE` (3 s) has passed quietly, the runtime sends a commit+push. A
  failure marks `needs_pull` and schedules a retry after `RETRY` (30 s), pulling
  first so a rejected (non-fast-forward) push can recover.
- **Sync worker**: `Sync` spawns one thread that adopts or creates the
  repository, then processes `Pull` / `CommitPush` jobs and returns
  `Result<(), String>` over a channel. The app never blocks on git. With no
  remote configured, no git process is ever started.
- On exit the runtime saves and calls `flush`, a synchronous best-effort push,
  so the last change is not lost when the process ends.
- No tokio: one slow operation plus one poll is enough for std threads.

## UI contract

- `ui::draw(frame, &Model, &Theme, data_path, remote)` renders container → body
  → footer → modal. It **only reads** `Model`.
- Below `40×8` it renders a centred "terminal too small" hint instead.
- `KANBAN_MIN_WIDTH = 100`: at or above it, `kanban` (three columns); below it,
  `sections` (vertical stack).
- Section headers: `TODAY` / `THIS WEEK <iso>/<weeks>` / `LATER`, each a label
  row plus a full-width rule.
- Colour is signal only: selected row, today, done/dropped states.
- Toast: a brief `saved to today` after capture, gone after ~3 s, never
  stealing the cursor.
- The footer's right side shows, in priority order: a write error, sync status
  (a blinking dot while syncing, `✗ sync failed · <reason>` on failure), then the
  toast.
- Help overlay: geometry lives in a few constants at the top of `modal.rs`; long
  paths and URLs wrap at their separators and right-align with the key column.
  A failed sync is shown as a **one-line reason**, never a raw git dump; the full
  error is left to `tasu sync`.

## CLI

- `tasu` → the TUI.
- `tasu add <title>...` → load + `settle` + `add` + save + **best-effort**
  commit/push (bounded; offline is fine and the exit code is still 0), printing
  what it captured.
- `tasu list` (alias `ls`) → load + `settle` and print the open tasks grouped by
  bucket. Read-only and local; `tasu sync` refreshes from the remote first.
- `tasu config` → print the resolved data dir, config file and remote.
- `tasu remote <url>` / `--clear` → probe, then save or clear (see Sync).
- `tasu sync` → one pull-then-push, printing the real git error on failure.
- `tasu update` → upgrade using however tasu was installed: `brew upgrade` for a
  Homebrew install, the official installer for a script install, `cargo install`
  for a Cargo install; otherwise it prints the right command. The method is
  inferred from the executable path and the installer's receipt, so no new
  dependency or download logic is needed.

Both front-ends share `domain` / `store` / `sync`.

## Settings

**Zero-config by default**: after `cargo install tasu` it runs purely local,
offline. Settings exist only for advanced use.

| Setting | Default | Override | Notes |
| --- | --- | --- | --- |
| Data directory | platform data dir `/tasu` | `TASU_DATA`, or config file | Holds `todos.json`; also the git working tree when syncing |
| Sync remote | none (local) | `TASU_REMOTE`, or config file | Git URL; with none set, git is never invoked |

- Config file: platform config dir `/tasu/config.json` (`dirs::config_dir()`),
  optional fields `data_dir` and `remote`. **Config is kept out of the data
  repository** so it never travels between machines.
- Precedence: environment > config file > default.
- `tasu remote <url>` **probes before it persists**: if `git ls-remote` fails
  (missing repository, no credentials/permission, no network) the remote is not
  written and any previous value is cleared, and the command exits non-zero.
  This avoids saving a remote that would be retried forever.

## Sync

Git single-writer model (see `PRODUCT.md`): `git -C <repo> pull --rebase
--autostash origin <branch>`, then `git add -A && git commit ... && git push
origin HEAD:refs/heads/<branch>`. With **no remote / not a repository** the whole
layer degrades silently to local and produces no git calls.

- **Branch contract**: every machine syncs on **one branch** so a host's
  `init.defaultBranch` (often `master` on Windows) never leaks into the data
  repo. The remote's default branch is adopted when it has one, otherwise
  `main`; `push` targets the explicit branch name and `pull` names
  `origin <branch>` (no upstream needed). An unreadable remote triggers no
  rename, so a transient failure cannot corrupt a healthy local branch.
- **First-connect merge**: when the local and remote boards both exist, the graft
  path calls `store::merge_files` to union and de-duplicate instead of letting
  the local side overwrite the remote. If one side is missing it is kept as is;
  if a schema cannot be understood it falls back to the local side rather than
  guessing. After the merge, normal single-writer pull/push resumes.
- **Error classification**: `sync::Failure::classify` maps git's stderr to
  `NotFound` / `Auth` / `Network` / `Other`, shared by the CLI message and the
  in-app footer/help.

## Testing

End-to-end first (ratatui `TestBackend` + temp directories + local bare
repositories):

- keypress → screen contents / `todos.json` / git history;
- time boundaries: day rollover, ISO-week rollover (including week 1 across a
  year), `settle` idempotence, frozen done/archived tasks;
- moving between buckets, undo back to the original bucket and position,
  restoring dropped tasks, the history `DONE`/`DROPPED` views;
- sync: silent when no remote, an offline push that does not block exit, the
  first-connect merge, and branch-name adoption.

Pure layers (domain, update, store, sync) also have unit tests, and the UI has
`insta` snapshot tests. Git-dependent tests run against real repositories so
they exercise the real binary.

> Rule of thumb: to isolate a subsystem for testing, first write down *how it
> could fail*, then write the code. Domain-isolation tests are reserved for hard
> cases like time boundaries; everything else is E2E.
