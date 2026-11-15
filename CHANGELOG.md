# Changelog

Notable changes to Backrank. The version rules are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## [Unreleased]

## [1.0.0] - 2026-11-15

The first stable release. Backrank meets its v1 definition of done: perft-verified move
generation, a 1,000-game stress run within its limits, a working test harness, and measured
strength.
The engine plays as in 0.3.0.

### Strength

- 56.9% against Stockfish at `UCI_Elo` 2000 over 240 games at 8+0.08, Elo +48 [+5, +93],
  for an estimated rating of about 2050. The previous run of the same engine scored 58.5%.
  The estimate is probably optimistic at this fast time control.

### Robustness

- 1,000 self-play games over clock and fixed move-time controls, from `1+0.01` to `st=0.05`:
  no crashes, illegal moves or protocol errors, and 2 time losses (0.2%), both with a fixed
  move time on a heavily loaded machine. Raise `Move Overhead` if a GUI's move-time games
  run short on a busy computer.

## [0.3.0] - 2026-11-15

### Added

- `backrank bench [depth]`: a fixed-depth search of 50 positions, printing each one's best
  move and node count and then the total as `<nodes> nodes <nps> nps`. The total is the same
  on every machine, so it fingerprints the search. At the default depth of 5 it is
  19,532,453 nodes.

## [0.2.0] - 2026-11-06

### Added

- Prebuilt binaries for macOS, Linux and Windows, on x86-64 and ARM64.
- `backrank --version`.
- The engine so far: bitboard move generation validated with perft, the UCI protocol, and
  alpha-beta search with iterative deepening, quiescence search, move ordering and time
  management, evaluating material and piece-square tables.

### Strength

- 58.5% against Stockfish at `UCI_Elo` 2000 over 240 games at 8+0.08, Elo +60 [+17, +105],
  for an estimated rating of about 2060. That estimate is probably optimistic at this fast
  time control.
