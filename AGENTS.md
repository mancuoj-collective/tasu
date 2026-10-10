# AGENTS.md

Guidance for agents working in this repository.

## Commits

Follow [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): subject`, with a body explaining the *why* for anything
non-trivial. Do not rewrite already-published history.

## Engineering

- **Tests travel with the change.** User-facing behaviour gets an end-to-end case
  in `tests/e2e.rs`; pure logic gets a unit test; a visual change updates the
  `insta` snapshots. Before committing run `cargo fmt`,
  `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`.
- **No backwards compatibility.** Only the author runs tasu, so delete compat
  scaffolding (old config locations, schema versions, migration shims) instead of
  keeping it.
- **Minimal dependencies.** No async runtime and no HTTP client; git is a
  subprocess. Ask before adding a crate.
- **Keep the layers pure.** `domain` takes `now` as an argument and does no I/O;
  the UI only reads the model. See `ARCHITECTURE.md`.

## Releases

- Bump the **minor** by one every release and never reach 1.0: `0.14.0` →
  `0.15.0` → …
- **Do not release unless explicitly asked.** Batch changes between releases.
- To release: bump `Cargo.toml`, move `CHANGELOG.md`'s `Unreleased` into a dated
  section, commit `chore(release): <version>`, tag `v<version>`, push the tag.

## Working style

- Discuss a change before making it — the author's ideas are not always right.
- Keep all UI chrome in English.
- Every failure must be visible to the user (a footer hint, a command error, or
  a sync status) — never silent.
- Prefer prebuilt, zero-friction installs (Homebrew tap + release installer).
