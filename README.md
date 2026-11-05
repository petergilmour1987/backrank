# Backrank Chess Engine

A chess engine for standard chess, written in Rust. It speaks the UCI protocol and is
developed against Stockfish on a difficulty ladder that gets harder as Backrank improves.

**Status:** early development. Move generation is complete and verified with perft, and
the engine speaks UCI, but for now it plays a random legal move: the real search is still
to come.
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
cargo run --release              # run the engine in UCI mode
```

### Using it as a UCI engine

Run with no arguments, `backrank` reads UCI commands on stdin and answers on stdout. To use
it in a chess GUI (Cute Chess, Arena, BanksiaGUI and so on), add
`target/release/backrank` as a UCI engine. To try it by hand:

```
$ cargo run --release
uci
id name Backrank 0.1.0
id author petergilmour1987
option name Move Overhead type spin default 20 min 0 max 5000
uciok
position startpos moves e2e4
go movetime 1000
info depth 1 score cp 0 nodes 20 nps 20000 time 0 pv g8f6
bestmove g8f6
quit
```

It supports the standard commands: `uci`, `isready`, `setoption`, `ucinewgame`,
`position`, `go` (with `wtime`, `btime`, `winc`, `binc`, `movestogo`, `depth`, `nodes`,
`movetime` and `infinite`), `stop` and `quit`. A bare `go` searches until `stop`.

| Option | Default | Effect |
|---|---|---|
| `Move Overhead` | 20 | Milliseconds kept in reserve per move for communication delays (0 to 5000). |

Two non-standard commands help with debugging:

| Command | Effect |
|---|---|
| `d` | Print the board, its FEN and its Zobrist key. |
| `go perft <depth>` | Perft from the current position, in Stockfish's output format. It runs in the background, so `stop` ends it early. |

If a `position` command is invalid (a bad FEN or an illegal move), the engine reports it
with `info string` and keeps the previous position.

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

`cargo test` includes UCI tests that run the real binary and talk to it over pipes. The
deeper perft tests take too long for every run, so they are ignored by default. Run them
after any change to move generation (they take a few seconds in release mode):

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

### The Stockfish ladder

Backrank's strength is measured by playing it against a ladder of ever-stronger
opponents, defined in [`tools/ladder.toml`](tools/ladder.toml):

| Rungs | Opponent |
|---|---|
| `random` | A pure random mover. |
| `blunder80` … `blunder10` | Stockfish at Skill Level 0, playing a random legal move instead 80% … 10% of the time (via the `sfproxy` helper). |
| `sf-elo1320` … `sf-elo3190` | Stockfish with `UCI_LimitStrength`. Its scale is roughly CCRL Blitz rating. |
| `sf-full` | Stockfish at full strength. |

```sh
cargo xtask ladder                      # play the current rung
cargo xtask ladder --climb              # keep going up while rungs are passed
cargo xtask ladder --rung sf-elo1500    # check a particular rung
cargo xtask ladder --note "add quiescence search"
```

A run plays 240 games at 8+0.08 (about 10 minutes against Stockfish). It prints the score,
the Elo difference with a 95% confidence interval, and the likelihood of superiority
(LOS). Against a rung with a rating, it also prints an estimated rating for Backrank.

Every run adds a row to [`results/ladder.csv`](results/ladder.csv), Backrank's progress
history. Backrank moves up a rung (recorded in `results/state.toml`) after scoring at
least 60% over at least 200 games on its current rung, with no time forfeits, illegal
moves or crashes of its own. Scoring below 40% on a rung it passed before prints a
regression warning. Backrank is never moved down automatically.

Before each run, Backrank is built and copied to `tools/engines/backrank-<commit>`, so
rebuilding during a run doesn't affect it.

| Option | Default | Effect |
|---|---|---|
| `--rung <name>` | current rung | Play a different rung. Only the current rung can promote. |
| `--climb` | off | After moving up, play the next rung too, until one isn't passed. |
| `--games <n>` | `240` | Number of games. Must be even. |
| `--tc <tc>` | `8+0.08` | Time control as seconds+increment. |
| `--concurrency <n>` | `8` | Games played at the same time. |
| `--seed <n>` | random | Opening order seed. Recorded in the CSV, so a run can be repeated. |
| `--note <text>` | none | Recorded in the CSV. |

### Calibrating the bottom rungs

```sh
cargo xtask calibrate                   # all blunder rungs, strongest first (1 to 1.5 hours)
cargo xtask calibrate --rung blunder10  # just one
```

Run it under `caffeinate -i cargo xtask calibrate` on a Mac. If the machine sleeps during
a match, the clocks keep running and games are lost on time. Calibrate then refuses the
result, and you have to rerun that rung.

The blunder rungs have no rating of their own. `calibrate` works down from `sf-elo1320`:
each rung plays its stronger neighbour, and its rating is that neighbour's rating plus the
measured difference. If a rung loses or wins every game, the gap can't be measured, so
its rating is recorded only as a bound and a warning suggests changing the blunder
values. Results go to [`results/calibration.toml`](results/calibration.toml). It takes
`--games`, `--tc` and `--concurrency` like `ladder`.

### Did a change help? (SPRT)

```sh
cargo xtask sprt                        # working tree vs HEAD
cargo xtask sprt --base HEAD~1          # vs any commit, tag or branch
```

This plays the working tree against an earlier version, using fastchess's sequential
probability ratio test. The test stops as soon as the result is clear: "H1 accepted"
means the change gains at least `--elo1`, and "H0 accepted" means it gains no more than
`--elo0`. The base version is built in a temporary git worktree, and the build is kept for
the next run. Results are added to `results/sprt.csv`.

| Option | Default | Effect |
|---|---|---|
| `--base <ref>` | `HEAD` | The version to compare against. |
| `--elo0`, `--elo1` | `0`, `10` | The two hypotheses, in normalized Elo. |
| `--alpha`, `--beta` | `0.05` | Error rates. |
| `--tc <tc>` | `8+0.08` | Time control. |
| `--max-games <n>` | `20000` | Give up without a verdict after this many games. |

### Self-play stress test

```sh
cargo xtask selfplay
```

This builds Backrank and plays it against itself through fastchess. The result doesn't
matter. The test passes only if every game ends normally: any crash, illegal move, time
forfeit or stalled game is reported as a failure. 200 games take a few seconds while
Backrank plays random moves.

| Option | Default | Effect |
|---|---|---|
| `--games <n>` | `200` | Number of games. Must be even. |
| `--tc <tc>` | `4+0.04` | Time control as seconds+increment. |
| `--concurrency <n>` | `8` | Games played at the same time. |

fastchess can also check the engine's UCI behaviour directly:

```sh
cargo build --release && tools/bin/fastchess --compliance target/release/backrank
```

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
tools/           ladder.toml, plus installed match tooling (bin/, books/, engines/ are gitignored)
results/         ladder and SPRT history (committed); pgn/ and logs/ hold match output (gitignored)
docs/            plans and design docs
```
