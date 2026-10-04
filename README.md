# tasu

[![Crates.io](https://img.shields.io/crates/v/tasu.svg)](https://crates.io/crates/tasu)
[![CI](https://github.com/mancuoj-collective/tasu/actions/workflows/ci.yml/badge.svg)](https://github.com/mancuoj-collective/tasu/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A terminal todo list that ages.

Tasks you record go into **today**. Whatever you don't finish sinks — today's
leftovers become **this week's**, and this week's leftovers become **later**.
There are no due dates, projects or tags; the list organizes itself by how
fresh each task is.

```
TODAY
──────────────────────────────────────
 ○ Nand2Tetris chapter 6
 ○ learn GPUI events
 THIS WEEK  41/53
──────────────────────────────────────
 ○ write the README
 LATER
──────────────────────────────────────
 ○ read the ratatui source
```

## Install

```bash
cargo install tasu
```

Or from source:

```bash
git clone https://github.com/mancuoj-collective/tasu.git
cd tasu
cargo install --path .
```

Requires Rust 1.88 or newer.

## Usage

```bash
tasu                 # open the app
tasu add "buy cat food"   # record a task without opening the app
tasu config          # show the resolved data dir, config file and sync remote
tasu remote <url>    # set the git sync remote (see below)
tasu sync            # run one pull-then-push now and print the result
```

| Key | Action |
| --- | --- |
| `q` / `Esc` | quit |
| `Ctrl`+`c` | quit from anywhere |
| `j` / `k` / `↑` / `↓` | move the cursor |
| `h` / `l` / `←` / `→` | jump to the previous / next bucket |
| `g` / `G` | go to the top / bottom |
| `space` / `Enter` | complete |
| `a` | add (goes into today) |
| `e` | edit the title |
| `t` | move to today |
| `[` / `]` | send the task to a nearer / farther bucket |
| `x` | drop (archive) |
| `c` | open history — `Tab` or `←`/`→` switches between done and dropped |
| `?` | help |

Narrow terminals stack the buckets vertically; terminals **100 columns or
wider** lay them out as three side-by-side columns.

Completing or dropping a task moves it to the history overlay (`c`), where
either can be restored to its original bucket with `Enter`, or filtered by
typing in the search box.

## Configuration

tasu is zero-config by default and **purely local** unless you point it at a
git remote. Both settings can be given as environment variables or in a small
config file.

| Setting | Default | Environment | Config file |
| --- | --- | --- | --- |
| Data directory | platform data dir `/tasu` | `TASU_DATA` | `data_dir` |
| Sync remote | none (local only) | `TASU_REMOTE` | `remote` |

The config file lives at the platform config directory, in `tasu/config.json`:

```json
{
  "data_dir": "/path/to/tasu-data",
  "remote": "git@github.com:you/tasu-data.git"
}
```

Environment variables take precedence over the file. With no `remote`, tasu
never invokes git.

## Syncing between machines (optional)

Sync is plain git against a single repository, so a private repo, a Gist or a
self-hosted server all work the same. Create a **private** repository and point
tasu at it:

```bash
tasu remote you/tasu-data      # GitHub shorthand → https://github.com/you/tasu-data.git
tasu                            # next start clones/syncs
```

`tasu remote` accepts a full URL, an SSH address (`git@…`), a local path, or a
GitHub `owner/repo` shorthand. After setting it, tasu checks that the remote is
reachable and that credentials work; over HTTPS this uses the token your git
credential helper already stores. If the check fails, it prints how to fix it.

The repository does not have to be empty: if it already has commits (say a
README), tasu adopts that history on first sync and pushes the local board on
top, keeping both.

`tasu remote` with no argument prints the current remote, and
`tasu remote --clear` turns syncing off. The data directory and sync remote are
also shown at the bottom of the in-app help (`?`).

On the first run tasu initializes the data directory as a git repository and
pushes. On another machine, an empty data directory is **cloned** from the
remote.

How it behaves inside the app:

- pulls when it starts;
- after each change, commits and pushes a few seconds later (debounced);
- shows a blinking dot in the footer while a sync is running (a warning-coloured
  dot if the last one failed);
- pushes once more on exit;
- if the network is down or the push fails, stays quiet and retries later
  (pulling first, so a rejected push can recover).

If syncing seems stuck, `tasu sync` runs it once and prints the real git error,
and `tasu config` shows which remote is in use.

`tasu add` pulls before it reads and pushes after it writes, so a machine that
only ever uses the CLI stays in sync too.

This is designed for a **single writer at a time** (your work machine and your
home machine, not both at once). There is no merge of conflicting edits; the
last write wins.

### Trying sync locally

You can validate the whole thing against a local bare repository:

```bash
# a stand-in for the remote
git init --bare /tmp/tasu-remote.git

# machine A
TASU_DATA=/tmp/tasu-a TASU_REMOTE=/tmp/tasu-remote.git tasu add "from A"

# machine B: clones A's history, then adds
TASU_DATA=/tmp/tasu-b TASU_REMOTE=/tmp/tasu-remote.git tasu add "from B"

# back on A: pulls B's change, then adds
TASU_DATA=/tmp/tasu-a TASU_REMOTE=/tmp/tasu-remote.git tasu add "back on A"

git --git-dir /tmp/tasu-remote.git log --oneline   # three sync commits
```

## Non-goals

On purpose, there is no: due dates, projects, tags, priorities, subtasks,
notes, notifications, recurrence, global hotkey, or multi-writer merge.

## Development

The pure layers (domain, update, store, sync) are covered by unit and
end-to-end tests, including a real git round trip:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

To eyeball the layout without a terminal UI, render sample boards to text:

```bash
cargo run --example preview
```

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
