# tasu

A terminal todo list manager built with [ratatui](https://ratatui.rs).

## Status

Early development. The core todo workflow is implemented: add, edit, toggle and
delete items in an in-memory list. Persistence is not implemented yet.

## Installation

```bash
cargo install tasu
```

Or from source:

```bash
git clone https://github.com/mancuoj-collective/tasu.git
cd tasu
cargo run --release
```

## Usage

| Key          | Action      |
| ------------ | ----------- |
| `q` / `Esc`  | Quit        |
| `Ctrl` + `c` | Quit        |
| `j` / `Down` | Move down   |
| `k` / `Up`   | Move up     |
| `space`      | Toggle done |
| `a`          | Add         |
| `e`          | Edit        |
| `d`          | Delete      |

In the add/edit prompt, `Enter` saves and `Esc` cancels. In the delete prompt,
`y` confirms and `n` / `Esc` cancels.

## Requirements

Rust 1.88 or newer.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
