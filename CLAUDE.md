# Backrank Chess Engine

A chess engine written in Rust. The crate is named `backrank` and builds both a
library (the engine) and a binary (the front end that talks to GUIs / the user).

## Layout

```
Cargo.toml          # single crate: [lib] + [[bin]], both named `backrank`
src/lib.rs          # engine library — all chess logic belongs here
src/main.rs         # thin binary; parses I/O and calls into the library
tests/              # integration tests (create when needed)
benches/            # benchmarks (create when needed)
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
cargo clippy --all-targets -- -D warnings   # lint; must be clean
cargo build --profile profiling # release + debug symbols, for profilers/flamegraphs
```

Before declaring work done: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
and `cargo test` must all pass.

## Conventions

- Rust edition 2024, stable toolchain (`rust-toolchain.toml`).
- No `unsafe` unless a measured performance win justifies it; document the invariant
  with a `// SAFETY:` comment.
- No new dependencies without a clear reason — the engine core should stay dependency-light.
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

Project scaffold only — no engine code yet. The v1 roadmap (phases, decisions,
Stockfish ladder, UCI spec, perft suite) is in `docs/plan-v1.md`; follow its phase order
and exit checks.
