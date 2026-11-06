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
docs/               # plans and design docs — v1: docs/plan-v1.md, releases: docs/plan-release.md
.github/workflows/  # ci.yml: the gates on every pull request to main; release.yml and build.yml:
                    # merging a new version builds, tags and publishes its GitHub Release
```

Keep `main.rs` thin: no chess logic in the binary. Anything worth testing lives in
the library so it can be exercised from unit and integration tests.

## Commands

```sh
cargo build                     # debug build
cargo build --release           # optimised build (LTO, codegen-units=1) — use for play/perft speed
cargo run --release             # run the engine binary
cargo run --release -- --version  # print the version
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
cargo xtask sprt [--base REF]   # does the working tree beat REF? (default: where the branch left main)
cargo xtask calibrate           # rate the blunder rungs below sf-elo1320 (1 to 1.5 hours)
cargo xtask bump <major|minor|patch> [--no-commit]  # raise the version, move changelog notes under it
cargo xtask check-release       # the version and strength-evidence rules (a CI gate)
cargo xtask verify-binary PATH  # run a built binary: right version, legal move
cargo run --release -- perft N [startpos | fen F] [moves ...]  # perft split by first move
```

Keep `README.md`'s usage guide current when commands or options are added or change.

Before declaring work done: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` must all
pass. Run them locally before pushing: pull requests must also pass them in CI.

`xtask` may use crates (clap, anyhow, sha2); the `backrank` crate itself is std-only.

## Git workflow

- Never commit to `main`: it is protected, and everything reaches it through a pull request.
  Work on a short-lived branch named `feat/`, `fix/`, `docs/`, `ci/` or `chore/`.
- A pull request can merge once the `gates` check passes (`.github/workflows/ci.yml`).
- Merge with `gh pr merge --merge` (or `--auto --merge`). Squash and rebase are disabled,
  because `results/*.csv` labels each run with a commit hash that must stay on `main`.
- If a pull request changes what ships (`src/`, root `Cargo.toml`, `rust-toolchain.toml`,
  `.cargo/`), its last commit is `cargo xtask bump <level>`, with notes in `CHANGELOG.md`
  under `[Unreleased]`. The README has the level rules. `check-release` enforces this.
- Merging a bump publishes a GitHub Release automatically, and published releases are immutable.
  Fix a bad release by shipping a patch, never by moving a tag.
- SPRT and ladder runs stay local; don't try to run them in CI. Search and eval changes must
  commit their `results/` rows, or carry the `no-strength-test` label if they can't alter play.
- Nothing publishes to a package manager, and `publish = false` stays. That work is deferred
  (`docs/plan-release.md` §4).

## Conventions

- Rust edition 2024. The toolchain is pinned in `rust-toolchain.toml`; update it in its own
  pull request.
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

Phases 0 to 5 are done (tooling, core types, move generation, UCI, Stockfish ladder, search and
evaluation); Backrank is on the `sf-elo2000` rung. The branch, gate and GitHub Release process
(`docs/plan-release.md`) is in place. Next is Phase 6 (hardening and v1.0.0). For any search or
eval change, follow the development loop in plan §8 on a branch: tests, then `caffeinate -i
cargo xtask sprt`, then `caffeinate -i cargo xtask ladder --note ...`, then `cargo xtask bump
<level>` and a pull request. Commit the code first, so runs are labelled with a clean hash, and
the `results/` rows afterwards. The ladder builds the working tree, so don't leave other edits
in it when a run starts. Stage explicit paths when committing. After changing move generation
or make/unmake, also run `cargo test --release --test perft -- --ignored`, and use `cargo xtask
perft-diff` to localise any mismatch. The v1 roadmap (phases, decisions, Stockfish ladder, UCI
spec, perft suite) is in `docs/plan-v1.md`; follow its phase order and exit checks.
