# Changelog

Notable changes to Backrank. The version rules are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## [Unreleased]

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
