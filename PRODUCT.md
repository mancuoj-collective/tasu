# tasu — product

> A flat, one-screen personal task list that layers itself by freshness —
> instant to capture, worth opening daily, restrained but coloured.

This is the product contract. It is not a feature list but a set of
**boundaries**: what tasu does, and deliberately does not do. The architecture
is derived from it — every module has to answer "which part of the product is
this for?".

## In one line

Capture costs nothing, planning costs nothing: whatever you jot down lands in
**today**, unfinished work sinks on its own, and all you do is work and tick.

## Core model

One task = **title + state + bucket**.

- **No dates, projects, tags, notes, priorities or subtasks.**
- Three states: `open` / `done` / `archived`.
- Three mutually exclusive buckets: **today** / **this week** / **later**.

### Capture

- New tasks go into **today** by default.
- Capture from the TUI with `a`, or from the shell with `tasu add "..."` without
  opening the TUI.

### Ageing (the core mechanism)

Tasks are a pipeline that ages against real time, settled **lazily by local
date** — only when you open or call tasu, so no daemon is needed:

- Each day: unfinished **today** → **this week**.
- Each week (Monday boundary): unfinished **this week** → **later**.
- **later** never ages further.

The result: today empties every day down to what you added or promoted; the
less you finish, the deeper it sinks.

### Completing and dropping

- **Complete** (`space` / `enter`): leaves the list and lands in the `DONE` view
  of history.
- **Drop** (`x`): a task you will not do lands in the `DROPPED` view. It is not a
  completion, and it is reversible.
- **History overlay** (`c`): `tab` switches between `DONE` and `DROPPED`,
  everything is kept and searchable, and `enter` restores any entry **to its
  original bucket and position**.
- There is no delete key: completing and dropping are both reversible.

## Interface

- **Narrow**: the three buckets stack vertically (`TODAY` / `THIS WEEK` /
  `LATER`).
- **Wide** (past a column threshold): they become three side-by-side columns
  separated by rules.
- **No borders**; hierarchy comes from section titles, full-width rules and
  indentation.
- A **cold palette**; colour is signal, not decoration: the selected row and
  `TODAY`.
- The `THIS WEEK` header shows **ISO week / weeks in the year** (e.g. `41/53`).
- Within a bucket, newest first.
- Capture does **not** steal focus: a footer toast `saved to today` flashes and
  disappears.

## Keys

| Key | Action |
| --- | --- |
| `j` `k` `↓` `↑` | move the cursor |
| `h` `l` `←` `→` | move between buckets (columns on a wide screen) |
| `g` `G` `Home` `End` | top / bottom |
| `space` `enter` | complete |
| `a` | capture (into today) |
| `e` | edit the title |
| `t` | promote to today |
| `[` `]` | send the task to the nearer / farther bucket |
| `x` | drop (into `DROPPED`) |
| `c` | history (`tab` switches `DONE` / `DROPPED`) |
| `?` | help |
| `q` `esc` | quit |
| `ctrl+c` | quit from any mode |

## Sync

**Single-writer git**, aimed at a work machine and a home machine that are
**not edited at the same time**.

- The data file lives in a git repository (the data directory doubles as the
  working tree; `TASU_DATA` overrides it and the remote is configurable).
- **One branch**: every machine syncs on the same branch (the remote's default,
  otherwise `main`), independent of the host's `init.defaultBranch`.
- **First connect never loses data**: when both the local and the remote boards
  have tasks they are **merged (union, deduplicated)** instead of the local side
  overwriting the remote; if either side is empty it is kept as is.
- The remote is probed before it is saved: unreachable / unauthorized / missing
  is **not saved, and clears any previous setting**, rather than leaving a config
  that always fails.
- Pull on start; after a change, debounce a few seconds then `add/commit/push`;
  one more push on exit.
- Offline or a failed push: stay quiet and retry later, without interrupting.
- **No concurrent merge**: under a single writer, last-write-wins is safe enough.

## Non-goals

Explicitly excluded, treated as non-existent in the architecture:

- multi-writer merge / CRDT
- system notifications
- recurring tasks
- priorities
- subtasks
- due dates
- projects / tags
- global hotkey
- realtime multi-device sync (git-based turn-taking only)
