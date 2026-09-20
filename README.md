# tasu

A terminal todo list manager built with [ratatui](https://ratatui.rs).

## Status

Early development. The app currently renders a static screen and supports quitting;
the todo functionality is not implemented yet.

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

| Key | Action |
| --- | --- |
| `q` | Quit |
| `Esc` | Quit |
| `Ctrl` + `c` | Quit |

## Requirements

Rust 1.88 or newer.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
