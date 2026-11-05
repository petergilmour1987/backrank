# Backrank Chess Engine

A chess engine for standard chess, written in Rust. It speaks the UCI protocol and is
developed against Stockfish on a difficulty ladder that gets harder as Backrank improves.

**Status:** early development. Move generation is complete and verified with perft, and
the match tooling works, but the engine can't play games yet (UCI comes next).
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
cargo run --release              # run the engine (for now it only prints its name)
```

### Perft

`perft` counts every legal move sequence to a given depth, which is how move generation
is tested. The output is split by first move and uses the same format as Stockfish's
`go perft`:

```sh
cargo run --release -- perft 5                                  # from the start position
cargo run --release -- perft 4 fen "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
cargo run --release -- perft 3 startpos moves e2e4 e7e5         # after some moves
```

The position follows UCI `position` syntax: `[startpos | fen <FEN>] [moves <move>...]`.
Timing goes to stderr, so stdout can be compared with Stockfish's directly.

## Checks

Run these before committing; all three must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The deeper perft tests take too long for every run, so they are ignored by default. Run
them after any change to move generation (they take a few seconds in release mode):

```sh
cargo test --release --test perft -- --ignored
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

### Finding move generation bugs

```sh
cargo xtask perft-diff --depth 5
cargo xtask perft-diff --fen "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1" --depth 6
```

This builds Backrank and compares its perft, move by move, with the Stockfish snapshot
from `setup`. If a move's count differs, it plays that move and compares again one ply
shallower. It stops at the exact position where a move is missing or extra, and prints
that position's FEN and the moves involved. It exits with an error if any difference is
found.

| Option | Default | Effect |
|---|---|---|
| `--depth <n>` | (required) | Perft depth to compare at. |
| `--fen <FEN>` | start position | Position to start from. |

## Project layout

```
src/             the engine: library (all chess logic) and a thin binary
tests/           integration tests (perft suite)
xtask/           development tooling behind `cargo xtask`
tools/           installed match tooling (gitignored, created by setup)
results/         match output (pgn/ and logs/ are gitignored)
docs/            plans and design docs
```
