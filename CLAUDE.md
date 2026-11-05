# Backrank Chess Engine

A chess engine written in Rust. The crate is named `backrank` and builds both a
library (the engine) and a binary (the front end that talks to GUIs / the user).

## Layout

```
Cargo.toml          # workspace root + the `backrank` package ([lib] + [[bin]], both `backrank`)
src/lib.rs          # engine library — all chess logic belongs here
src/types/          # Square/File/Rank, Bitboard, Color/PieceType/Piece, CastlingRights, Move
src/attacks/        # leaper tables (compile time) and magic bitboards for sliders
src/board/          # Position: bitboards + mailbox, FEN, Zobrist, make/unmake, attack queries
src/movegen/        # pseudo-legal generation, MoveList, legal_moves / find_legal_move
src/perft.rs        # perft and divide
src/eval/           # static evaluation: material + piece-square tables
src/search/         # iterative deepening alpha-beta, quiescence, ordering, time management
src/uci/            # UCI loop: stdin thread -> engine thread -> per-`go` search thread
src/main.rs         # thin binary; parses I/O and calls into the library
tests/              # integration tests: perft.rs (deep depths #[ignore]d), uci.rs (runs the binary)
benches/            # benchmarks (create when needed)
xtask/              # dev tooling crate, run as `cargo xtask <cmd>` (alias in .cargo/config.toml)
xtask/src/bin/sfproxy.rs  # Stockfish Skill 0 that plays random moves with probability p
tools/ladder.toml   # the Stockfish difficulty ladder (committed)
tools/              # bin/, books/, engines/ are installed by `cargo xtask setup` (gitignored)
results/            # ladder.csv, sprt.csv, state.toml, calibration.toml are committed history;
                    # pgn/ and logs/ hold match output (gitignored)
docs/               # plans and design docs — v1 plan: docs/plan-v1.md
```

Keep `main.rs` thin: no chess logic in the binary. Anything worth testing lives in
the library so it can be exercised from unit and integration tests.

## Commands

```sh
cargo build                     # debug build
cargo build --release           # optimised build (LTO, codegen-units=1) — use for play/perft speed
cargo run --release             # run the engine binary
cargo test                      # all tests
cargo test --release            # use for slow tests (e.g. deep perft)
cargo fmt                       # format (config: rustfmt.toml)
cargo clippy --workspace --all-targets -- -D warnings   # lint; must be clean
cargo build --profile profiling # release + debug symbols, for profilers/flamegraphs

cargo xtask setup               # install fastchess, opening book, Stockfish snapshot into tools/
cargo xtask smoke               # harness check: Stockfish Skill 0 vs full strength via fastchess
cargo xtask perft-diff --depth N [--fen F]   # compare perft with Stockfish, find the bad move
cargo xtask selfplay            # Backrank vs itself; fails on any crash/illegal move/time loss
cargo xtask ladder [--climb] [--note "..."]  # play the current ladder rung, record in results/
cargo xtask sprt [--base REF]   # does the working tree beat REF? (SPRT, default [0, 10])
cargo xtask calibrate           # rate the blunder rungs below sf-elo1320 (1 to 1.5 hours)
cargo run --release -- perft N [startpos | fen F] [moves ...]  # perft split by first move
```

Keep `README.md`'s usage guide current when commands or options are added or change.

Before declaring work done: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` must all
pass. Run them locally: the GitHub workflow is manual-only for now, to save Actions minutes.

`xtask` may use crates (clap, anyhow, sha2); the `backrank` crate itself is std-only.

## Conventions

- Rust edition 2024, stable toolchain (`rust-toolchain.toml`).
- No `unsafe` unless a measured performance win justifies it; document the invariant
  with a `// SAFETY:` comment.
- The `backrank` crate uses only the standard library; tooling dependencies go in `xtask`.
- Correctness before speed: move generation must be validated with perft against known
  node counts before optimising it.
- Performance-sensitive code: prefer fixed-size types and avoid heap allocation in the
  search / move-generation hot paths.
- Public items in the library get doc comments.
- Tests: unit tests in `#[cfg(test)] mod tests` beside the code; cross-module tests in `tests/`.

## Chess domain notes

- Target protocol: UCI (Universal Chess Interface) for GUI communication.
- Positions are exchanged as FEN; moves in UCI long algebraic notation (e.g. `e2e4`, `e7e8q`).
- Remember the rules that are easy to get wrong: castling rights/through-check, en passant
  (including discovered checks), promotion, the fifty-move rule and threefold repetition.

## Status

Phases 0 to 5 are done (tooling, core types, move generation, UCI, Stockfish ladder, search
and evaluation); Backrank is on the `sf-elo2000` rung. Next is Phase 6 (hardening and
v1.0.0). For any search or eval change, follow the development loop in plan §8: tests, then
`caffeinate -i cargo xtask sprt`, then `caffeinate -i cargo xtask ladder --note ...`, then
commit. Commit the code first, so runs are labelled with a clean hash, and the `results/`
rows afterwards. The ladder builds the working tree, so don't leave other edits in it when
a run starts. Stage explicit paths when committing. After changing move
generation or make/unmake, also run `cargo test --release --test perft -- --ignored`, and
use `cargo xtask perft-diff` to localise any mismatch. The v1 roadmap (phases, decisions,
Stockfish ladder, UCI spec, perft suite) is in `docs/plan-v1.md`; follow its phase order
and exit checks.
