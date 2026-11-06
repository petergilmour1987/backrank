# Changelog

Notable changes to Backrank. The version rules are in the README's development workflow.

## [Unreleased]

## [0.2.0] - 2026-11-06

### Added

- `backrank --version`.
- The engine so far: bitboard move generation validated with perft, the UCI protocol, and
  alpha-beta search with iterative deepening, quiescence search, move ordering and time
  management, evaluating material and piece-square tables.

### Strength

- 58.5% against Stockfish at `UCI_Elo` 2000 over 240 games at 8+0.08, Elo +60 [+17, +105],
  for an estimated rating of about 2060. That estimate is probably optimistic at this fast
  time control.
