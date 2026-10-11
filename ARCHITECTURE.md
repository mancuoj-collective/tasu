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
5. **Minimal dependencies.** No HTTP client and no async runtime: git is a
   system command, file changes are detected by polling mtime, and shell
   completions come from `clap_complete`.

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
  main.rs            entry: parse the CLI, run the loop, spawn the exit flush
  cli.rs             clap definition
  config.rs          settings: data dir / remote (env > config file > default)
  command/
    mod.rs           one-shot task commands (add, done, drop, move, list, sync)
                     all changing commands share `change_board`
    render.rs        pure plain-text rendering of lists and history
    remote.rs        `tasu remote`: set / clear, refusal message
    update.rs        `tasu update`: install method detection and upgrade
  domain/
    task.rs          Task / TaskState / Bucket
    board.rs         Board: collection + operations + per-bucket queries
    settle.rs        pure settle(&mut Board, now)
  store/mod.rs       load / atomic save / mtime / merge_files
  sync/
    mod.rs           Sync worker thread + result channel; pull, commit+push
    git.rs           the only place git is spawned: C locale, deadlines, stderr
    repo.rs          adopt / clone / graft, branch resolution, .gitignore
    failure.rs       Failure: classify git stderr for the footer and `tasu remote`
  app/
    model.rs         Model = Board + UiState
    action.rs        Action / Effect enums
    update.rs        update(&mut Model, Action) -> Vec<Effect>
    tracker.rs       SyncTracker: pure sync state machine + `due` scheduler
    runtime.rs       I/O half: Store + Sync worker, applies the tracker's decisions
  session.rs         the interactive loop: draw, collect, read one input, apply
  ui/
    mod.rs           Chrome, width-based layout selection, footer
    keys.rs          footer hints and help rows, one table
    theme.rs         cold palette
    sections.rs      narrow vertical stack
    kanban.rs        wide three columns
    components.rs    task rows, selected-row bar, headers (ISO week), toast
    modal.rs         add/edit and history overlays
    help.rs          help overlay
    details.rs       full text of a write error or board damage, opened with `!`
    text.rs          pure text wrapping for the overlays
```

## Data contract (on disk)

A single `todos.json` in the data directory (the platform data dir by default,
overridable with `TASU_DATA`).

```json
{
  "tasks": [
    {
      "title": "Nand2Tetris chapter 6",
      "state": "open",              // open | done | archived
      "bucket": "today",            // today | week | later
      "bucket_since": "2026-10-04T09:12:00.123+08:00",
      "created_at":   "2026-10-04T09:12:00.123+08:00",
      "completed_at": null,
      "archived_at":  null,
      "updated_at":   null           // last user change; absent until the first
    }
  ]
}
```

Key decisions:

- **No id.** Single writer, no concurrent merge, and operations address tasks by
  position. Bucket order is derived from `created_at` (with nanoseconds), so
  **restoring a task means moving it back to its bucket and resetting
  `bucket_since`** — ordering falls back into place with no extra bookkeeping.
  Because `created_at` *is* the identity, `Board::add` guarantees it is unique:
  a colliding `now` is nudged forward by a nanosecond, so two tasks can never
  silently collapse into one on a later merge.
- **`bucket` survives `done` / `archived`**, so undo and restore have an
  "original bucket" to return to.
- All times are `chrono::DateTime<Local>`.
- `store::merge_files(base, other)` merges two serialized boards task by task
  (identity = `created_at`), and returns `Err` naming the side that could not be
  read, so a merge never silently drops tasks.
- **One atomic writer for `todos.json`.** The app thread (`Runtime`) is the only
  thing that writes the file (through `Store`, temp file + rename). The sync
  worker never writes it: a pull reads the remote board out of the fetched
  commit and hands it back, and the app merges it in and saves. So a background
  pull can never clobber a save the user just made.
- **An unreadable board is never silent.** `Store::load` returns `Loaded`
  (`board` + `Option<Damage>`); a file that cannot be parsed is moved aside
  (`.corrupt-<unix-seconds>`) and a read failure is reported the same way. The
  TUI shows the damage in the footer and the CLI warns on stderr, so a board that
  appears to have vanished is always explained.

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
    updated_at: Option<DateTime<Local>>,  // last user change, for merges
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

A pull is not carried as an `Effect`: the runtime sends one background pull, and
the remote board it returns comes back as `Action::Merge`, which the model merges
into the current board and saves. **mtime detection** is separate — it catches a
*truly external* write (a one-shot `tasu add`, a hand edit) as `Action::Reload`.
The runtime tracks `last_mtime` and refreshes it right after its own writes,
which is what separates "our write" from "an external one".
- The editor latches the task's `created_at` when it opens, so a `Reload` that
  reorders the rows mid-edit still renames the task the user opened.

## Main loop and concurrency

- The loop: draw → drain sync results and check mtime → wait up to `TICK`
  (200 ms) for a key → `update` → apply effects.
- **`Runtime`** (in `app/runtime.rs`, part of the library) owns the `Store`, the
  optional `Sync` worker and the mtime baseline. It is the only place that
  touches the disk, the clock or the network.
- **`SyncTracker`** (in `app/tracker.rs`) holds the sync state (`dirty`,
  `last_change`, `retry_at`, `in_flight`, `needs_pull`, the footer status). It
  takes `Instant`s as arguments and never does I/O, so every transition is
  unit-tested with plain values: a job's outcome, a queued pull or push, a
  failure's footer reason.
- **Debounced push**: the decision is the pure
  `due(&SyncState, DEBOUNCE) -> Due` — a function of a few flags and elapsed
  times. `Runtime` just performs it: once nothing is in flight and `DEBOUNCE`
  (3 s) has passed quietly, it sends a commit+push. A failure marks `needs_pull`
  and schedules a retry after `RETRY` (30 s), pulling first so a rejected
  (non-fast-forward) push can recover.
- **Failures never reach stderr during the TUI.** Sync and store problems go to
  the footer; a save that fails on quit is printed after the terminal is restored.
- **Sync worker**: `Sync` spawns one thread that adopts or creates the
  repository, then processes `Pull` / `CommitPush` jobs and returns a
  `JobOutcome` (`Pulled` / `Pushed` / `Failed`) over a channel. It reports its
  one-shot repository-setup outcome first (`poll_init`), so a broken setup is
  shown in the footer instead of looking idle. The app never blocks on git. With
  no remote configured, no git process is ever started.
- On exit the runtime saves and calls `flush`, a synchronous best-effort push,
  so the last change is not lost when the process ends.
- No tokio: one slow operation plus one poll is enough for std threads.

## UI contract

- `ui::draw(frame, &Model, &Theme, &Chrome)` renders container → body
  → footer → modal. It **only reads** `Model`; the single value it returns is the
  help overlay's clamped scroll offset, which the caller stores (so key handling
  always starts from a valid position without the renderer mutating state).
- Below `40×8` it renders a centred "terminal too small" hint instead.
- `KANBAN_MIN_WIDTH = 100`: at or above it, `kanban` (three columns); below it,
  `sections` (vertical stack).
- Section headers: `TODAY` / `THIS WEEK <iso>/<weeks>` / `LATER`, each a label
  row plus a full-width rule.
- Colour is signal only: selected row, today, done/dropped states.
- Toast: a brief `saved to today` after capture, gone after ~3 s, never
  stealing the cursor.
- The footer's right side shows, in priority order: a write error, sync status
  (a blinking dot while syncing, `✗ <summary> · ! details` on failure), then
  the toast. Every failure reads the same way.
- Every failure — a write error, a board that could not be read, a sync
  failure — keeps a `Problem` (`summary` + `detail`). The footer shows only the
  short `summary` with a `· ! details` affordance, so a long IO error, a backup
  path or a git dump never crowds it out; pressing `!` opens the details
  overlay, which wraps the full `detail` and scrolls like help. `!` is inert
  when nothing is wrong. A sync failure's overlay detail ends with the
  `run tasu sync` retry hint. Every problem clears when its cause does (a
  successful save, a successful sync).
- When `todos.json` could not be read, the footer's **left** side replaces the
  key hints with the damage summary, so the sync status on the right stays
  visible.
- Help overlay: geometry lives in a few constants at the top of `help.rs`; long
  paths and URLs wrap at their separators and right-align with the key column.
  The details overlay reuses that wrapping.
- Quitting returns to the shell at once: the board is already saved, and the
  final commit+push is spawned as a detached `tasu flush` rather than awaited.

## CLI

- `tasu` → the TUI.
- `tasu add <title>...` → **best-effort** pull, then load + `settle` + `add` +
  save + **best-effort** commit/push (bounded; offline is fine and the exit code
  is still 0). A failed pull or push prints one `tasu: ... failed` line on
  stderr, so nothing is silent. Then it prints
  what it captured.
- `tasu list [bucket] [--json]` (alias `ls`) → load + `settle` and print the open
  tasks grouped by bucket. Read-only and local; `tasu sync` refreshes first.
- `tasu history [done|dropped] [--json]` → print completed and dropped tasks
  (both by default).
- `tasu done <title>` / `tasu drop <title>` / `tasu move <bucket> <title>` →
  mutate an **open** task identified by its exact title (erroring on none or
  several matches, since tasks carry no id), then save and best-effort push.
- `tasu config [--json]` → print the resolved data dir, config file and remote.
- `tasu remote <url>` / `--clear` → probe, then save or clear (see Sync).
- `tasu sync` → one pull-then-push, printing the real git error on failure.
- `tasu update` → upgrade using however tasu was installed: `brew upgrade` for a
  Homebrew install, the official installer for a script install, `cargo install`
  for a Cargo install; otherwise it prints the right command. The method is
  inferred from the executable path and the installer's receipt, so no new
  dependency or download logic is needed.
- `tasu completions <shell>` → print a completion script for the given shell.

The everyday subcommands carry a short `visible_alias` (`a`, `ls`, `h`, `d`,
`x`, `mv`), so clap lists them inline under `tasu --help`; the occasional ones
have none.

Both front-ends share `domain` / `store` / `sync`.

## Settings

**Zero-config by default**: after `cargo install tasu` it runs purely local,
offline. Settings exist only for advanced use.

| Setting | Default | Override | Notes |
| --- | --- | --- | --- |
| Data directory | platform data dir `/tasu` | `TASU_DATA`, or config file | Holds `todos.json`; also the git working tree when syncing |
| Sync remote | none (local) | `TASU_REMOTE`, or config file | Git URL; with none set, git is never invoked |

- Config file: `$XDG_CONFIG_HOME/tasu/config.json` (or `~/.config/tasu/config.json`),
  optional fields `data_dir` and `remote`. It lives here, **not** in the platform
  config dir, because on macOS and Windows that equals the data dir — which would
  drop `config.json` inside the synced working tree, where a checkout could
  overwrite it.
- Precedence: environment > config file > default.
- `tasu remote <url>` **probes before it persists**: if `git ls-remote` fails
  (missing repository, no credentials/permission, no network) the remote is not
  written and any previous value is cleared, and the command exits non-zero.
  This avoids saving a remote that would be retried forever.

## Sync

Git single-writer model (see `PRODUCT.md`). **Pull never rebases** — rebasing the
JSON board conflicts as soon as two machines touch it, and then the push is
rejected and the repo is stranded. Instead `pull` fetches into a private ref and
compares histories: if the remote is reachable from `HEAD` there is nothing to
do; otherwise it reads the remote board out of the fetched commit and moves the
branch to the remote tip with `git reset --soft` — which touches neither the
index nor the working tree — then hands the board back as `Pulled::Merge`. The
**caller** merges it task by task (`Board::merged_with`, identity = `created_at`,
terminal state wins, then the later change time) and saves; the next
`commit_push` (`git add -A && git commit ... && git push origin
HEAD:refs/heads/<branch>`) then fast-forwards from the remote tip and publishes
the union. The worker therefore never writes `todos.json`, so it cannot race the
app's own save. With **no remote** the whole layer degrades silently to local
and produces no git calls; when a remote is set but the repository cannot be
adopted or created, that outcome is reported once through the worker's init
channel rather than swallowed.

- **Branch contract**: every machine syncs on **one branch** so a host's
  `init.defaultBranch` (often `master` on Windows) never leaks into the data
  repo. The remote's default branch is adopted when it has one, otherwise
  `main`; `push` targets the explicit branch name and `pull` names
  `origin <branch>` (no upstream needed). An unreadable remote triggers no
  rename, so a transient failure cannot corrupt a healthy local branch. The
  reconcile needs an `ls-remote`, so its result is remembered in the repo's own
  Git config (`tasu.remote`): it runs once per remote, not on every launch.
- **First-connect merge**: when the local and remote boards both exist, the graft
  path calls `store::merge_files` to merge them task by task instead of letting
  the local side overwrite the remote. The merge runs **before** the remote's
  tree is adopted (`git reset --hard`), so a board that cannot be read returns an
  error with the local files — `todos.json` included — still untouched. If one
  side is missing it is kept as is. After the merge, normal single-writer
  pull/push resumes.
- **Error classification**: `sync::Failure::classify` maps git's stderr to
  `NotFound` / `Auth` / `Network` / `Other`. `SyncTracker` keeps the classified
  phrase as the footer summary and the whole git error as the details `detail`,
  so the footer stays one line and `!` shows the rest.

## Testing

End-to-end first (ratatui `TestBackend` + temp directories + local bare
repositories):

- keypress → screen contents / `todos.json` / git history: capture, rename,
  complete, drop, restore from either history view, bucket moves, cursor jumps
  across buckets, history search, the toast and per-mode hints, the empty and
  too-small screens, and cursor-visible scrolling;
- time boundaries: day rollover, ISO-week rollover (including week 1 across a
  year), `settle` idempotence, frozen done/archived tasks;
- the `Runtime` reloads the board when the file changes underneath it, and does
  not mistake its own write for an external one;
- the real built binary (`CARGO_BIN_EXE_tasu`) is driven for `list --json`, an
  ambiguous title being refused, and a non-zero exit from a failed `tasu sync`;
- sync: silent when no remote, the first-connect merge, branch-name adoption,
  two machines converging, and a diverged merge keeping the later rename.

Pure layers (domain, update, store, sync) also have unit tests, as does the
sync tracker (the `due` scheduler, outcome handling, retry backoff, pull-before-push), and
the UI has `insta` snapshot tests. Git-dependent tests run against real
repositories, so they exercise the real git binary.

> Rule of thumb: to isolate a subsystem for testing, first write down *how it
> could fail*, then write the code. Domain-isolation tests are reserved for hard
> cases like time boundaries; everything else is E2E.
