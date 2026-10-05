# tasu

[![Crates.io][crates-src]][crates-href]
[![CI][ci-src]][ci-href]
[![License][license-src]][license-href]

**A terminal todo list that ages.**

Tasks go into **today**. Whatever you don't finish sinks — today's leftovers
become **this week's**, and this week's become **later**. There are no due
dates, projects or tags; the list organizes itself by how fresh each task is.

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

Prebuilt binaries, no Rust toolchain needed.

**macOS / Linux**

```bash
brew install mancuoj/tap/tasu
```

Or without Homebrew:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mancuoj-collective/tasu/releases/latest/download/tasu-installer.sh | sh
```

**Windows (x86_64)** — in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/mancuoj-collective/tasu/releases/latest/download/tasu-installer.ps1 | iex"
```

This installs `tasu.exe` into `%USERPROFILE%\.cargo\bin` and adds it to your
PATH; open a new terminal afterwards. Or download
[`tasu-x86_64-pc-windows-msvc.zip`](https://github.com/mancuoj-collective/tasu/releases/latest/download/tasu-x86_64-pc-windows-msvc.zip)
and put `tasu.exe` somewhere on your PATH.

**With Cargo** (Rust 1.88 or newer)

```bash
cargo install tasu
```

Or build from a checkout:

```bash
git clone https://github.com/mancuoj-collective/tasu.git
cd tasu
cargo install --path .
```

## Usage

```bash
tasu                        # open the app
tasu add "buy cat food"     # record a task without opening the app
tasu list [today|week|later]# print the open tasks
tasu done "buy cat food"    # complete a task
tasu drop "old idea"        # drop (archive) a task
tasu move later "someday"   # send a task to another bucket
tasu history [done|dropped] # print history (both views by default)
tasu config                 # print the data dir, config file and sync remote
tasu remote <url>           # set the git sync remote (optional)
tasu sync                   # pull then push once, and report the result
tasu update                 # upgrade tasu the way it was installed
tasu completions bash       # print a shell completion script
```

The everyday commands have short aliases, shown in `tasu --help`: `a` (add),
`ls` (list), `h` (history), `d` (done), `x` (drop), `mv` (move).

Every command that changes something prints what it did, and `list`, `history`
and `config` take `--json` for scripting.

Tasks carry no id, so `done`, `drop` and `move` match an open task by its
**exact title**; if none or several match, they error rather than guess.

`tasu update` detects how tasu was installed and runs the matching upgrade —
`brew upgrade mancuoj/tap/tasu`, the official installer, or `cargo install tasu`
— so the binary and the package manager that owns it stay in agreement. For an
unknown install it prints the right command instead of guessing.

### Keys

| Key | Action |
| --- | --- |
| `j` `k` `↓` `↑` | move the cursor |
| `h` `l` `←` `→` | previous / next section |
| `g` `G` `Home` `End` | top / bottom |
| `space` `Enter` | complete |
| `a` | add (goes into today) |
| `e` | edit the title |
| `t` | move to today |
| `[` `]` | send to the nearer / farther bucket |
| `x` | drop (archive) |
| `c` | history |
| `?` | help |
| `q` `Esc` | quit |
| `Ctrl`+`c` | quit from anywhere |

Narrow terminals stack the buckets vertically; **100 columns or wider** lays
them out as three side-by-side columns.

`c` opens the history overlay. `Enter` restores a completed or dropped task to
its original bucket, `Tab` (or `←`/`→`) switches between *done* and *dropped*,
and typing filters the list.

## Sync between machines (optional)

tasu is local-first: with no remote it never invokes git. Point it at a
**private** repository to sync, and it behaves like a single writer that commits
as you go.

```bash
tasu remote you/tasu-data   # GitHub shorthand → https://github.com/you/tasu-data.git
tasu                        # next start clones / syncs
```

`tasu remote` accepts a full URL, an SSH address (`git@…`), a local path, or a
GitHub `owner/repo` shorthand. It verifies the remote **before** saving it; if
the check fails nothing is written (and any previous remote is cleared), so tasu
never gets stuck retrying an unreachable repository. With no argument it prints
the current remote, and `tasu remote --clear` turns syncing off.

- **First run** clones into an empty data directory. If the remote already has
  history (say a README), tasu adopts it and pushes the local board on top.
- If **both sides** already have a board, they are merged (union, exact
  duplicates dropped) — neither machine loses tasks.
- **One branch** everywhere: the remote's default branch if it has one,
  otherwise `main`. A machine's `init.defaultBranch` never leaks into the data
  repo.
- In the app, tasu pulls on start and pushes a few seconds after each change
  (debounced). A failure shows `✗ <reason> · run tasu sync` in the footer;
  `tasu sync` prints the full git error. Quitting hands any last push to a
  detached process and returns to the shell immediately.
- `tasu add` pulls before it reads and pushes after it writes, so a CLI-only
  machine stays in sync too.
- Designed for **one writer at a time** (your work machine *or* your home
  machine, not both editing at once). There is no merge of conflicting edits.

To try it against a local bare repository:

```bash
git init --bare /tmp/tasu-remote.git

TASU_DATA=/tmp/tasu-a TASU_REMOTE=/tmp/tasu-remote.git tasu add "from A"
TASU_DATA=/tmp/tasu-b TASU_REMOTE=/tmp/tasu-remote.git tasu add "from B"        # clones A
TASU_DATA=/tmp/tasu-a TASU_REMOTE=/tmp/tasu-remote.git tasu add "back on A"     # pulls B

git --git-dir /tmp/tasu-remote.git log --oneline
```

## Configuration

Environment variables take precedence over the config file. With no `remote`,
tasu never invokes git.

| Setting | Default | Environment | Config file |
| --- | --- | --- | --- |
| Data directory | platform data dir `/tasu` | `TASU_DATA` | `data_dir` |
| Sync remote | none (local only) | `TASU_REMOTE` | `remote` |

The config file lives at `$XDG_CONFIG_HOME/tasu/config.json` (i.e.
`~/.config/tasu/config.json`):

```json
{
  "data_dir": "/path/to/tasu-data",
  "remote": "git@github.com:you/tasu-data.git"
}
```

It is kept **outside the data repository**, so it never travels between
machines (and a git checkout can never overwrite it).

## Non-goals

On purpose, there is no: due dates, projects, tags, priorities, subtasks,
notes, notifications, recurrence, global hotkey, or multi-writer merge.

## Development

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The pure layers (domain, update, store, sync) are covered by unit and
end-to-end tests, including a real git round trip. To eyeball the layout without
a terminal, render sample boards to text:

```bash
cargo run --example preview
```

[`PRODUCT.md`](PRODUCT.md) is the product contract and
[`ARCHITECTURE.md`](ARCHITECTURE.md) describes how it is built.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).

<!-- Badges -->

[crates-src]: https://img.shields.io/crates/v/tasu?style=flat&colorA=18181b&colorB=1f6feb
[crates-href]: https://crates.io/crates/tasu
[ci-src]: https://img.shields.io/github/actions/workflow/status/mancuoj-collective/tasu/ci.yml?branch=main&style=flat&colorA=18181b&colorB=1f6feb&label=CI
[ci-href]: https://github.com/mancuoj-collective/tasu/actions/workflows/ci.yml
[license-src]: https://img.shields.io/github/license/mancuoj-collective/tasu?style=flat&colorA=18181b&colorB=1f6feb
[license-href]: https://github.com/mancuoj-collective/tasu/blob/main/LICENSE
