# Backrank Chess Engine

A chess engine for standard chess, written in Rust. It speaks the UCI protocol and is
developed against Stockfish on a difficulty ladder that gets harder as Backrank improves.

**Status:** early development. The match tooling works; the engine itself can't play yet.
The roadmap and progress are in [`docs/plan-v1.md`](docs/plan-v1.md).

## Requirements

- **Rust**, via [rustup](https://rustup.rs). `rust-toolchain.toml` selects the stable
  toolchain with `rustfmt` and `clippy`, and rustup installs it on first use.
- **Stockfish**, for the match tooling: `brew install stockfish` on macOS, or any build
  on your `PATH`.
- **`curl`, `tar` and `unzip`**, used by `cargo xtask setup` (preinstalled on macOS).
- **macOS or Linux** on x86-64 or ARM64 for the match tooling, which uses prebuilt
  fastchess binaries for those platforms.

## Building and running

```sh
cargo build --release            # optimised build: target/release/backrank
cargo run --release              # run the engine
```

## Checks

Run these before committing; all three must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The GitHub Actions workflow runs the same checks, but for now it only runs when started by
hand (Actions tab, or `gh workflow run CI`).

## Match tooling

Development tasks are run with `cargo xtask <command>`. Use `cargo xtask --help` or
`cargo xtask <command> --help` for every option.

### One-time setup

```sh
cargo xtask setup
```

This installs everything the matches need into `tools/` (gitignored):

| Installed | Location | Source |
|---|---|---|
| fastchess v1.8.2-alpha, the match runner | `tools/bin/fastchess` | GitHub release, SHA-256 checked |
| `noob_3moves.epd`, 150,932 balanced openings | `tools/books/` | official-stockfish/books at a pinned commit, SHA-256 checked |
| A snapshot of your Stockfish | `tools/engines/stockfish` | copied from `stockfish` on your `PATH` |

Stockfish is copied rather than used in place, so upgrading it with your package manager
doesn't silently change the opponent between test runs.

Running `setup` again skips anything already installed, and repairs anything that is
missing, broken or no longer matches the pinned version.

| Option | Effect |
|---|---|
| `--force` | Reinstall everything. Use it to pick up a newer Stockfish after upgrading. |
| `--stockfish <path>` | Snapshot a specific Stockfish binary instead of the one on `PATH`. |

### Smoke test

```sh
cargo xtask smoke
```

This checks that the match plumbing works by playing Stockfish at `Skill Level 0` against
full-strength Stockfish. It takes about 15 seconds. It passes when full strength scores at
least 90% and every game ends normally (no time forfeits, crashes or illegal moves).

| Option | Default | Effect |
|---|---|---|
| `--games <n>` | `20` | Number of games. Must be even: each opening is played once from each side. |
| `--tc <tc>` | `2+0.02` | Time control as seconds+increment. |
| `--concurrency <n>` | `8` | Games played at the same time. |

Each run saves its games to `results/pgn/` and the fastchess log to `results/logs/`. Both
directories are gitignored.

## Project layout

```
src/             the engine: library (all chess logic) and a thin UCI binary
xtask/           development tooling behind `cargo xtask`
tools/           installed match tooling (gitignored, created by setup)
results/         match output (pgn/ and logs/ are gitignored)
docs/            plans and design docs
```
