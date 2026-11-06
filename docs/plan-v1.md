# Backrank Chess Engine — v1 Plan

Status: **draft for review** · Written 2026-11-05

v1 is a correct UCI engine for standard chess (no variants). It has a classical
alpha-beta search and a simple evaluation. It comes with an automated harness that plays it
against Stockfish on a difficulty ladder that gets harder as the engine improves.

---

## 1. Goals and definition of done

v1 is done when all of these hold:

1. **Correct move generation.** Every perft position in §7.2 matches at full depth, using
   the release-mode ignored tests.
2. **Robust UCI.** Over a 1,000-game stress run there are 0 crashes, 0 illegal moves and
   0 protocol errors, and fewer than 0.5% time losses. The run mixes clock time controls
   (including bullet `1+0.01`) with `movetime` games.
3. **Measured strength.** The engine scores **≥ 50% against the `sf-elo1500` rung** at
   the ladder time control. The stretch goal is `sf-elo1600`. For comparison, Rustic
   Alpha 1, which has almost the same feature set (see §3), is rated **1675 on CCRL Blitz**.
4. **Working test harness.** `cargo xtask ladder` runs a full ladder match unattended in about
   10 minutes. It records the result in `results/ladder.csv` and moves the engine up a rung
   when it has earned it.
5. **Tagged release.** `v1.0.0` is tagged and its final ladder result is recorded.

Non-goals for v1: variants (including Chess960, now and later), pondering, multi-threaded
search, transposition table, opening book inside the engine, tablebases, NNUE.

## 2. Decisions

| Topic | Decision | Why |
|---|---|---|
| Board | Bitboards (12 piece boards + occupancy) plus a 64-square mailbox | Standard approach. Rustic, Stockfish and most of chessprogramming.org use it. The mailbox gives O(1) "what is on this square". |
| Sliding pieces | Magic bitboards, "fancy" variant (per-square shift and offset into one shared table, ~800 KiB) | The de facto standard. Magic numbers come from a fixed-seed trial-and-error search at startup (well under a second) and can be baked into constants later. |
| Legality | Pseudo-legal generation, then `make` and reject if our own king is attacked | Simple, and correct by construction. A fully legal generator (pins, check masks) is a later optimisation. |
| Make/unmake | Make/unmake with an undo stack, not copy-make | Lets the incrementally updated Zobrist key and history serve repetition detection. |
| Engine dependencies | **Std only** in the `backrank` crate | Threads use `std::sync::mpsc`. Zobrist keys and magic search use our own SplitMix64 or xorshift PRNG. |
| Tooling dependencies | Crates are allowed in `xtask` (`clap`, `anyhow`, `serde`, `toml`) | Tooling is not shipped. |
| Match runner | **fastchess** (prebuilt `fastchess-mac-arm64.tar`, v1.8.2-alpha) driven by a Rust `cargo xtask` | fastchess runs games in parallel, handles opening books and gives pentanomial/SPRT statistics. The xtask owns the ladder logic and the record-keeping. |
| Test budget | About 10 min per ladder run: 240 games at `8+0.08`, concurrency 8 | 240 games gives roughly ±40 Elo (95%) near a 50% score. That is enough for decisions about moving up a rung. |
| Square mapping | Little-endian rank-file (LERF): a1 = 0, h1 = 7, h8 = 63 | The most common mapping on chessprogramming.org, which makes reference code easy to compare. |

## 3. Reference points

**Rustic** (Rust, bitboards + magics; https://rustic-chess.org) is the closest existing
engine to this plan. Its version history shows what each feature is worth. The ratings are
CCRL Blitz 2m+1s; the per-feature gains were measured in self-play.

| Version | Features | Rating |
|---|---|---|
| Alpha 1 | Bitboards, magic move generation, UCI, alpha-beta, quiescence, MVV-LVA, check extension, material + PSTs | **1675** |
| Alpha 2 | + transposition table (TT cut-offs +42, TT move ordering +103) | 1815 |
| Alpha 3 | + killer moves (+56), PVS (+55) | 1865 |
| 3.1.x | + tapered and tuned eval (+248 in self-play) | — |

The Rustic book has two lessons this plan follows:

- Prove the baseline is bug-free before adding features ("Do not begin to extend the engine
  with new features until the baseline … has proven to be bug-free").
- Only about 60% of a self-play Elo gain survives against other engines.

Our v1 is roughly Rustic Alpha 1 plus PV-move ordering and repetition detection.

**chessprogramming.org** guidance this plan follows:

- Get a complete, bug-free board first. Castling and repetition are the hard rules.
- Verify move generation with perft.
- Start with negamax, iterative deepening and material-only evaluation.
- Cap search time at `remaining/20 + increment/2`.
- Accept or reject search and eval changes with SPRT, not with test-position suites.
- Play every opening twice with colours reversed.

## 4. Architecture

### 4.1 Repository layout (end of v1)

```
Cargo.toml              # workspace root + the `backrank` package
.cargo/config.toml      # alias: xtask = "run --quiet --package xtask --"
src/
  main.rs               # CLI entry: no args → UCI loop; `perft`, `bench` subcommands
  lib.rs
  types/                # Square, File, Rank, Color, PieceType, Piece, Bitboard, CastlingRights, Move
  board/                # Position: state, FEN parse/print, Zobrist, make/unmake, invariants
  movegen/              # leaper tables, magics, attacks, pseudo-legal gen, MoveList, perft
  eval/                 # material + piece-square tables
  search/               # iterative deepening, negamax αβ, quiescence, ordering, time mgmt, PV
  uci/                  # command parser, engine loop, output formatting
tests/                  # perft.rs, uci.rs (spawns the binary), search.rs (mates, draws)
xtask/                  # dev tooling crate (setup, ladder, sprt, perft-diff, calibrate)
  src/bin/sfproxy.rs    # "blunder" wrapper around Stockfish for sub-1320 rungs (§6.2)
tools/
  ladder.toml           # ladder rung definitions (committed)
  bin/  books/  engines/  # fastchess, opening books, snapshot binaries (gitignored)
results/
  ladder.csv            # one row per ladder run — the engine's progress history (committed)
  state.toml            # current rung (committed)
  pgn/  logs/           # game records and fastchess logs (gitignored)
docs/
  plan-v1.md
```

### 4.2 Core types

- **`Bitboard(u64)`** gets bit operators, `pop_lsb`, `count` and an iterator over squares.
- **`Square(u8)`** covers 0..63 in LERF order. `Color`, `PieceType` and `Piece` are small
  enums, so wrapper types keep a `Square` from being mixed up with a `Piece`.
- **`Move(u16)`** packs the from square (6 bits), the to square (6 bits) and 4 flag bits:
  quiet, double push, O-O, O-O-O, capture, en passant, 4 promotions, 4 promo-captures.
  The captured piece is read from the mailbox and stored in the undo record.
- **`MoveList`** is a fixed array of 256 moves plus a length. No heap allocation in
  movegen or search; the maximum number of legal moves in any position is 218.
- **`Position`** holds:
  - 12 piece bitboards, plus occupancy per colour,
  - the mailbox, `[Option<Piece>; 64]`,
  - side to move, castling rights (4 bits) and the en passant square,
  - the halfmove clock and fullmove number,
  - the Zobrist key,
  - an undo stack and a key history for repetition detection.

  Castling rights are updated through a 64-entry mask table. The en passant square is set
  only when an enemy pawn could actually capture there, so Zobrist keys and repetition
  match what FIDE counts as the same position.
- **Debug invariants.** In debug builds, `Position::check_invariants()` runs after every
  make and unmake. It checks that the mailbox and bitboards agree, that the incrementally
  updated Zobrist key equals a from-scratch recomputation, and that each side has exactly
  one king.

### 4.3 Threads (std only)

```
stdin thread ──UciCommand──▶  main/engine thread  ──spawn──▶ search thread
 (parse lines)   mpsc        owns Position + history       (cloned Position,
                              answers isready instantly     Arc<AtomicBool> stop,
                              stop → set flag, join          prints info + bestmove)
```

- The stdin thread parses each line into a `UciCommand` enum and sends it over `mpsc`.
  On EOF it sends `Quit`.
- The main thread handles commands. It never blocks on search, so `isready` is answered
  immediately even mid-search.
- A `go` spawns a search thread with a clone of the position. `stop` and `quit` set the
  flag and join the thread.
- Output goes only through a single `uci::out` module, which writes and flushes one line at
  a time. Diagnostics go to stderr, never stdout.

## 5. Phases

Each phase ends with an exit check. Work on the next phase starts only after the check passes.

### Phase 0 — Workspace and tooling foundation

- Convert the repo to a workspace: root package `backrank` plus a member `xtask`. Add the
  `.cargo/config.toml` alias.
- Add `cargo xtask setup`. It:
  - downloads `fastchess-mac-arm64.tar` (v1.8.2-alpha) into `tools/bin/`,
  - downloads and unzips `noob_3moves.epd` (balanced 3-move openings) from
    https://github.com/official-stockfish/books into `tools/books/`,
  - copies the Homebrew `stockfish` binary to `tools/engines/stockfish` so that a
    `brew upgrade` cannot silently change the opponent. Its version is read over UCI
    (`id name`) whenever it is needed.
- Add `cargo xtask smoke`: 20 games of Stockfish Skill 0 vs full-strength Stockfish
  through fastchess.
- Add a GitHub Actions CI workflow on ubuntu: `fmt --check`, `clippy -D warnings`, `test`.
  For now it is manual-only (`workflow_dispatch`) to save Actions minutes, and the checks
  are run locally.
- Add `tools/bin/`, `tools/books/`, `tools/engines/`, `results/pgn/` and `results/logs/`
  to `.gitignore`.

**Exit check:** `cargo xtask setup && cargo xtask smoke` works, and full-strength Stockfish
scores about 100%. This also confirms that options with spaces in their names, like
`Skill Level`, reach Stockfish correctly.

**Done 2026-11-05:** both downloads are pinned to a version or commit and checked by SHA-256.
The smoke run scored 20/20 for full strength in 15 s, with every game ending normally.

### Phase 1 — Core types and position

- Build the types in §4.2, the attack-free parts of `Position`, Zobrist keys from a
  fixed-seed PRNG, FEN parse and serialize (with proper errors for bad FEN), and an ASCII
  board printer.
- Unit tests: FEN round-trips for all the perft positions; bad FENs are rejected; square
  and bitboard helpers work.

**Exit check:** FEN round-trip tests pass, and clippy and fmt are clean.

**Done 2026-11-05:** types in `src/types/`, `Position` with FEN and Zobrist keys in `src/board/`,
and SplitMix64 in `src/rng.rs`. Zobrist keys are generated at compile time. FEN parsing
accepts 4–6 fields and validates material, castling rights and en passant. 37 unit tests
cover this.

### Phase 2 — Move generation and perft

- Precomputed knight, king and pawn attack tables.
- Magic bitboards: relevant-occupancy masks, a magic search (Romstad-style
  `rand & rand & rand` candidates with collision checking), and a shared attack table
  behind a `OnceLock`.
- `is_square_attacked`, pseudo-legal generation for every piece including castling
  (not out of, through or into check), en passant and promotions, plus `make`/`unmake`.
- `perft`/`divide`, exposed as `backrank perft <depth> [startpos | fen <FEN>] [moves …]`,
  with output in Stockfish's format (`e2e4: 20` … `Nodes searched: N`). The non-standard UCI
  command `go perft <n>` moves to Phase 3, which is where the UCI loop arrives.
- **`cargo xtask perft-diff --fen <F> --depth <D>`** runs `go perft` on both Backrank and
  Stockfish and compares the counts for each move. On a mismatch it plays that move and
  recurses one ply shallower, ending with the exact position and the move that is missing
  or extra. This turns a perft mismatch from hours of debugging into minutes.
- A property test: at every node of a depth-3 perft over all suite positions, make then
  unmake must restore the position exactly, including the Zobrist key.
- Carried over from Phase 1:
  - Once attack detection exists, `from_fen` must also reject positions where the side
    *not* to move is in check. This also covers adjacent kings.
  - `make_move` must set the en passant square with the same "a pawn of the side to move
    could capture" rule that `from_fen` uses. Share one function between them, and test that
    after a double push `key()` equals `from_fen(to_fen()).key()`. The rule is pseudo-legal:
    a pinned capturer still counts. That is a small, consistent difference from FIDE's
    definition of a repeated position.

**Exit check:** the full perft suite in §7.2 passes. As a soft speed target, startpos
perft 6 should take under about 5 s in release mode.

**Done 2026-11-05:**
- Every count in §7.2 matched on the first full run. Startpos perft 6 took 1.7 s (about
  70 million nodes/s on the M4 Pro), and the whole deep suite runs in about 3 s as release
  tests.
- The make/unmake property test walks 280,000 positions at depth 3. At each one it checks
  the invariants, checks the key against a rebuild from FEN, and checks that unmake
  restores the position exactly.
- `perft-diff` was tested by planting an en passant bug, which it traced to the exact
  position and missing move.
- The Phase 1 carry-overs are done: `from_fen` rejects positions where the side not to
  move is in check, and FEN parsing and `make_move` share `can_capture_en_passant`.

### Phase 3 — UCI protocol (first playable engine)

- Implement the threading model in §4.3 and the full command set in §7.1. A search stub
  plays a **random legal move**, so the protocol is complete before real search exists.
- Non-standard debug commands: `d` (print the board, FEN and key) and `go perft`.
- Carried over from the Phase 2 review:
  - `main` already calls `attacks::init()` at startup, so the magic tables are never built
    during a search. Keep it that way.
  - `make_move` assumes a pseudo-legal move. Debug builds check the position after every
    make/unmake, which makes debug-mode search slow. If Phase 5 tests suffer, gate that
    check (for example behind a cargo feature).
- FEN parsing is strict. If `position fen …` is rejected, report the error with `info string`
  and keep the previous position, so the engine never ends up with a half-applied position.
- Integration tests in `tests/uci.rs` spawn the binary and check:
  - `uci` → `id …`/`uciok`
  - `isready` → `readyok`, including while a `go infinite` is running
  - `position … moves …` + `go depth 1` → a legal `bestmove`
  - `stop` → exactly one `bestmove`
  - garbage input is ignored without a crash
  - `quit` exits promptly

**Exit check:** the random mover completes 200 fastchess games against itself with no
illegal moves, crashes or disconnects.

**Done 2026-11-05:**
- The UCI loop is in `src/uci/`, and the stand-in search is in `src/search/`. Its
  `Limits`/`Info`/`Score` interface is what Phase 5's real search must keep.
- `cargo xtask selfplay` played 200 games in 5 s, all ending normally (50.5%), with an
  empty fastchess warning log.
- `fastchess --compliance` passes all 40 of its checks. Those checks require an `info`
  line with a score before `bestmove`, so the stand-in search reports one.
- `tests/uci.rs` has 19 tests that run the binary over pipes. They passed 25 runs in a row
  after one timing assumption in a test was fixed.
- Fixed from the Phase 3 review:
  - stdin is decoded leniently, so a non-UTF-8 byte no longer ends the session;
  - `go perft` runs on the search thread and can be stopped;
  - "search until `stop`" is decided from the final `Limits` (bare `go`, `go mate`, or
    only the other side's clock);
  - `Limits.start` is taken when `go` arrives;
  - the search thread has a 64 MiB stack;
  - `selfplay` also fails on any fastchess log warning.

### Phase 4 — Stockfish match harness and ladder

- `tools/ladder.toml` holds the rungs (§6.2). `results/state.toml` holds the current rung.
- `sfproxy` is a UCI pass-through to Stockfish Skill 0. Before forwarding each `go`, it
  decides with probability *p* to play a uniformly random legal move instead. It gets the
  legal moves from Stockfish's own `go perft 1`. This gives the bottom of the ladder
  smooth difficulty steps between pure random and Stockfish's weakest setting.
- **`cargo xtask ladder [--rung N] [--games 240] [--tc 8+0.08] [--concurrency 8] [--climb]`**
  1. Builds Backrank in release mode and copies the binary to
     `tools/engines/backrank-<git-sha>[-dirty]`.
  2. Writes the fastchess command line from the rung definition and runs it with each
     opening played twice with colours swapped.
  3. **Parses the PGN itself** (`Result`, `Termination`, colours) to count wins, draws and
     losses, and to separately count time forfeits, illegal moves and crashes, which are
     treated as bugs and not just losses.
  4. Computes the score, Elo difference with a 95% confidence interval, LOS, and, against
     calibrated rungs, an estimated rating.
  5. Appends a row to `results/ladder.csv` and moves the engine up a rung if it qualifies.
     With `--climb` it keeps going until a rung is not passed.
- **`cargo xtask calibrate`** is a one-off run that plays neighbouring sub-1320 rungs
  against each other to estimate their ratings, anchored to `sf-elo1320`.
- **`cargo xtask sprt --base <git-ref>`** builds the base ref in a temporary worktree and
  runs fastchess SPRT for new vs base (`elo0=0 elo1=10 alpha=0.05 beta=0.05`, `8+0.08`).
  It is used to accept or reject each search or eval change from Phase 5 onward.
- The stats module (Elo formula, confidence interval, LOS) gets unit tests against known
  values.
- Carried over from the Phase 0 review: score each run from its own fresh PGN file, tally
  results per opponent pair (`Tally::between`), and pass fastchess `-srand <seed>` (recorded
  in the CSV) so a run's openings can be reproduced.

**Exit check:** the random mover scores about 50% against the `random` rung (this checks
the harness for bias) and about 0% against `sf-elo1320`. Both rows are recorded in
`results/ladder.csv`.

**Done 2026-11-05:**
- Random mover vs `random`: 48.5% (Elo -10 [-27, +7]) in 8 s. A 2,000-game bias check
  gave 49.9% (Elo -1 [-7, +5]): the harness is unbiased.
- Random mover vs `sf-elo1320`: 0/240 in about 4 minutes. That makes the random mover
  rated at most 561 (rule-of-three bound).
- All three rows are in `results/ladder.csv`.
- `sprt --base HEAD`, the random mover against itself, accepted H0 after 4,489 games in
  75 s.
- Calibration (240 games per rung at 8+0.08, Stockfish 19) rated the blunder rungs
  `blunder10` 1063, `blunder25` 778, `blunder40` 498, `blunder60` 236, `blunder80` −21 and
  `random` −268. Each scored 16–19% against the rung above, so the rungs are evenly
  spaced, about 250–285 Elo apart. Each step is uncertain by about ±60 Elo, and the error
  adds up down the chain.
- A first attempt lost the `blunder60` match to 32 time forfeits while the Mac slept.
  Calibrate refused to record a rating from it; run long matches under `caffeinate -i`.
- Stockfish 19's source confirms the `UCI_Elo` scale: it was fitted to games against the
  Stash engine, and Skill 0..19 covers roughly CCRL Blitz 1320–3190.
- Changes from the original design:
  - `sfproxy` uses Backrank's own perft-verified move generator for its random moves,
    instead of Stockfish's `go perft 1`. Before playing a move itself, it syncs with
    Stockfish via `isready`, so replies stay in order.
  - It is built with a new `tools` cargo profile (release without fat LTO).
  - Calibrated ratings live in `results/calibration.toml`, so the hand-written
    `tools/ladder.toml` stays free of generated data.
  - A rung isn't passed if Backrank lost any game by time forfeit, illegal move or crash.
- The Phase 0 carry-overs are done: each run uses a fresh PGN, results are tallied per
  opponent pair (`Tally::between`), and the `-srand` seed is recorded in the CSV.
- Fixed from the Phase 4 review:
  - The opponent's faults, an incomplete match or a non-standard time control now block
    promotion, and the opponent's faults are recorded.
  - Whitewashes use rule-of-three 95% bounds instead of a point estimate, and calibrate
    only carries a bound down the chain in its own direction.
  - A calibrated rating is ignored if the rung's blunder value has changed, with warnings
    for a different Stockfish, time control or neighbour rating.
  - Dirty builds are labelled `<sha>-dirty-<hash of the diff>`, and snapshots and state
    files are written atomically.
  - `sprt` checks that both engines answer UCI, and records faults and the seed.
  - `sfproxy` keeps a queue of who sent each request, hides `Skill Level`, logs when it
    can't follow the position, and reports Stockfish's real depth-1 score with its random
    moves. So fastchess logs only Stockfish's own Skill Level warnings.

### Phase 5 — Search and evaluation

Each step lands with its own tests, an SPRT against the previous step, and a ladder run.
The ladder runs record how much each feature is worth.

1. **Material eval + fixed-depth negamax alpha-beta** (fail-soft). Mate scores are
   `±(MATE − ply)` and are reported as `score mate N`. Checkmate and stalemate are
   detected when there are no legal moves.
2. **Iterative deepening and time management.** Carried over from the Phase 3 review:
   - Infinite mode must be handled inside the iterative-deepening loop. On reaching the
     maximum depth, or a forced mate, wait for `stop` before returning. The stand-in
     "search, then wait" won't carry over.
   - Measure time from `Limits.start`, and subtract `Move Overhead` from both the clock
     allocation and `movetime`, saturating at zero.
   - Treat `movestogo 0` as 1.
   - Add `seldepth` to `Info`.
   - `searchmoves` and `mate` are still ignored.

   Search time is allocated as
   `remaining/20 + inc/2` (with `movestogo` used when given), minus `Move Overhead`. No
   new iteration starts after half the allocation has been used. The search aborts at the
   full allocation; the stop flag and clock are checked every 2,048 nodes. `movetime`,
   `depth`, `nodes` and `infinite` are all supported. The engine prints an `info` line
   after each iteration and always completes depth 1, so it always has a move.
3. **Draw detection:** repetition (searching Zobrist keys back to the last irreversible
   move), the 50-move rule, and insufficient material. The keys are already in
   `Position`'s undo history (`Undo.key`); add an accessor or an `is_repetition` method there.
   Any move not freshly generated in the current position must pass an `is_pseudo_legal`
   check before `make_move`. In v1 nothing needs it, since the PV move only reorders
   generated moves; a transposition table will.
4. **Quiescence search:** stand-pat plus captures and queen promotions.
5. **Move ordering:** the PV move from the previous iteration first (triangular PV
   table), then captures ordered by MVV-LVA, then quiet moves.
6. **Check extension:** extend the search by 1 when in check, so quiescence never starts
   while in check.
7. **Piece-square tables:** Michniewski's *Simplified Evaluation Function*, with values
   P 100, N 320, B 330, R 500, Q 900 and separate middlegame and endgame king tables using
   the page's endgame rule. The score is from the side to move's perspective.

Tests in `tests/search.rs`: a handful of mate-in-1 and mate-in-2 positions, plus "do not
claim a draw when winning" and "do take a draw by repetition when losing". These check
correctness, not strength; chessprogramming.org warns that test suites correlate poorly
with playing strength.

**Exit check:** each step passes SPRT or is justified in its commit message, and the
ladder result for each step is recorded.

**Done 2026-11-05:** every step passed SPRT, and all ladder runs are in `results/ladder.csv`.

| Step | Commit | SPRT vs previous step | Ladder (240 games per rung at 8+0.08) | Est. rating |
|---|---|---|---|---|
| 1. Material, fixed depth 4 | `9dcdeb0` | 149–0–0 | passed `random` to `blunder10`; 25.8% vs `sf-elo1320` | 1137 |
| 2. Iterative deepening, time | `33d7716` | +368 [+315, +439] | 59.0% vs `sf-elo1320` | 1383 |
| 3. Draw detection | `87f0dcb` | +38 [+17, +60] | 58.3% vs `sf-elo1320` | 1378 |
| 4–5. Quiescence, move ordering | `47cf1f7` | +364 [+310, +436] | passed 1320, 1400; 52.5% vs `sf-elo1500` | 1517 |
| 6. Check extension | `61742b9` | +129 [+97, +162] | passed 1500; 45.2% vs `sf-elo1600` | 1567 |
| 7. Piece-square tables | `e6defc3` | +712 [+586, +inf] | passed 1600–1900; 54.6% vs `sf-elo2000` | 2032 |

The estimate comes from the last rung played in each run, and the interval is about ±45.

- The v1 goal and stretch goal (§1) are both met well before Phase 6. The ratings are
  probably optimistic: Stockfish's `UCI_Elo` scale is fitted to CCRL Blitz, not to 8+0.08,
  and it wasn't checked against any other engine. Self-play gains also overstate real
  ones: step 3's +38 didn't show against Stockfish at all.
- Steps 4 and 5 were merged. Quiescence without capture ordering took 3.5 million nodes
  and 264 ms for depth 1 on Kiwipete, which is a time-forfeit risk.
- No `is_pseudo_legal` check was needed. The PV move only reorders freshly generated
  moves, so it can never be illegal. A transposition table will need one (§9).
- The Mac must not sleep during a match. Use `caffeinate -i`.
- The Phase 5 review found no correctness defects. Its strength ideas change how the
  engine plays, so each needs an SPRT, and they are listed in §9.

### Phase 6 — Hardening and v1.0.0

- `backrank bench` searches a fixed set of positions at a fixed depth and prints total
  nodes and NPS. The node count acts as a fingerprint of search behaviour and goes in the
  commit message of any change that alters search.
- A stress run (the definition-of-done item 2 conditions) covering time-loss and
  edge-case behaviour, followed by fixes.
- A manual check in a GUI (Cute Chess or BanksiaGUI on macOS).
- Final ladder climb, the README updated with the estimated rating, and the `v1.0.0` tag.

## 6. The Stockfish ladder

### 6.1 Stockfish's own strength controls (Stockfish 19, verified via `uci` output)

- `Skill Level` (0–20, default 20). Stockfish searches with MultiPV 4 and deliberately
  picks a weaker move, chosen when it reaches a search depth tied to the level.
- `UCI_LimitStrength` + `UCI_Elo` (1320–3190) maps a rating onto a fractional skill level.
  According to Stockfish's source comments, Skill 0..19 corresponds roughly to
  CCRL Blitz 1320..3190. *Check this against the Stockfish 19 source in Phase 4.*
- **Nothing weaker than about 1320 exists.** Our engine starts far below that: the random
  mover in Phase 3, and material-only search early in Phase 5. That is why `sfproxy`
  provides the bottom rungs.

### 6.2 Rungs (`tools/ladder.toml`)

| # | Name | Opponent | Approx. rating |
|---|---|---|---|
| 0 | `random` | sfproxy *p* = 1.0 (pure random mover) | uncalibrated |
| 1 | `blunder80` | sfproxy *p* = 0.8 over Skill 0 | from calibration |
| 2 | `blunder60` | *p* = 0.6 | from calibration |
| 3 | `blunder40` | *p* = 0.4 | from calibration |
| 4 | `blunder25` | *p* = 0.25 | from calibration |
| 5 | `blunder10` | *p* = 0.1 | from calibration |
| 6 | `sf-elo1320` | `UCI_LimitStrength=true`, `UCI_Elo=1320` | ~1320 |
| 7–12 | `sf-elo1400` … `sf-elo1900` | `UCI_Elo` in steps of 100 | ~nominal |
| 13+ | `sf-elo2000` … `sf-elo3000`, `sf-elo3190`, `sf-full` | steps of 200 up to 3000, then 3190, then unlimited | ~nominal |

Every rung has `Threads=1` and `Hash=16`, and plays at the ladder time control.

### 6.3 Rules

- **Moving up:** score **≥ 60% over ≥ 200 games**. That is about +70 Elo, roughly 3.4
  standard errors above an even score.
- **Regression flag:** scoring below 40% on a rung that was previously passed prints a
  warning. Rungs are never moved down automatically, because a regression should be
  investigated rather than hidden.
- **Estimated rating** against calibrated rungs: `rung_elo + 400·log10(S/(1−S))`. It is
  reported with its confidence interval. Because our time control is faster than the one
  Stockfish was calibrated at, treat the number as approximate. The time series is what
  matters.
- **`results/ladder.csv` columns:**
  - date, git sha, dirty flag, bench node count
  - rung, Stockfish version, time control, games
  - W, D, L, score, Elo difference ± CI, LOS, estimated rating
  - time forfeits, illegal moves, crashes, notes

## 7. Specifications

### 7.1 UCI command set (v1)

**GUI → engine:**

| Command | Handling |
|---|---|
| `uci` | `id name Backrank <version>`, `id author …`, options, `uciok` |
| `isready` | `readyok`, immediately, even during search |
| `ucinewgame` | Reset game state. Never *rely* on it, because some GUIs don't send it. |
| `setoption name <id> [value <x>]` | Names may contain spaces: read up to `value`. Options: `Move Overhead` (spin, default 20 ms). `Hash` is not offered until a TT exists. |
| `position (startpos \| fen <6 fields>) [moves <m1> … ]` | Rebuild from scratch each time and replay the moves, keeping Zobrist history for repetition. |
| `go [wtime btime winc binc movestogo depth nodes movetime infinite]` | All times in ms. `searchmoves`, `ponder` and `mate` are parsed and ignored. A bare `go` means infinite. |
| `stop` | Finish and send `bestmove`. |
| `quit` | Stop the search and exit. |
| `debug`, `register`, `ponderhit`, unknown | Ignored. |

**Engine → GUI:**

- `info depth D seldepth S score (cp X | mate N) nodes N nps N time T pv m1 m2 …`
- `bestmove <move>`, or `bestmove 0000` when there is no legal move.

**Rules that are easy to get wrong:**

- Every `go` gets exactly one `bestmove`, including when `stop` arrives instantly.
- With `go infinite`, no `bestmove` is sent until `stop`.
- Moves use long algebraic notation: castling is the king's move (`e1g1`), and promotions
  have a lowercase suffix (`e7e8q`).
- Input is parsed tolerantly: any whitespace is accepted, and unknown tokens and
  commands are ignored.
- stdout carries only protocol lines. Debug output goes to stderr.

### 7.2 Perft suite

Counts are from chessprogramming.org *Perft Results*. The deepest count for each position
was checked against Stockfish 19's `go perft`.

| Position | FEN | Fast tests (default) | Deep tests (`--ignored`, release) |
|---|---|---|---|
| Start | `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1` | d1–4: 20, 400, 8,902, 197,281 | d5 4,865,609 · d6 119,060,324 |
| Kiwipete | `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1` | d1–3: 48, 2,039, 97,862 | d4 4,085,603 · d5 193,690,690 |
| Pos 3 | `8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1` | d1–5: 14, 191, 2,812, 43,238, 674,624 | d6 11,030,083 |
| Pos 4 | `r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1` | d1–4: 6, 264, 9,467, 422,333 | d5 15,833,292 |
| Pos 4 (mirrored) | `r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1` | same as Pos 4 | same as Pos 4 |
| Pos 5 | `rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8` | d1–3: 44, 1,486, 62,379 | d4 2,103,487 · d5 89,941,194 |
| Pos 6 | `r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10` | d1–3: 46, 2,079, 89,890 | d4 3,894,594 · d5 164,075,551 |

Once these pass, a larger EPD perft suite can be added, with `perft-diff` used to find the
cause of any mismatch.

## 8. Development loop (from Phase 5 on)

```
implement → cargo test (+ --release -- --ignored if movegen changed)
          → cargo xtask sprt --base main          # did it gain? (self-play)
          → cargo xtask ladder                     # how strong vs Stockfish now?
          → commit code + results/ladder.csv row (bench node count in message)
```

## 9. After v1 (v2 roadmap, in expected value order)

Small search ideas from the Phase 5 review, each to be tested with SPRT:

- ~~In quiescence, don't stand pat when in check: search all evasions, and score mate if
  there are none.~~ Tried and reverted: H0 accepted, Elo -25 [-44, -6] over 1,005 games at
  2+0.02. It costs more depth than its accuracy is worth; a version limited to a few plies
  might still be worth trying.
- When time runs out, use a partly completed iteration if its first root move finished,
  instead of discarding it.
- Don't search underpromotion captures in quiescence, and generate captures directly
  there rather than filtering all moves.

The larger features:

1. Transposition table with TT-move ordering and a `Hash` option. This was Rustic's
   largest single gain. TT moves can be hash collisions, so add `is_pseudo_legal` first.
2. Killer moves and the history heuristic.
3. Principal variation search and aspiration windows.
4. Null-move pruning, late move reductions and futility pruning.
5. Tapered evaluation (PeSTO-style middlegame and endgame tables), then Texel tuning.
6. Evaluation terms: pawn structure, mobility, king safety.
7. Lazy SMP, pondering, MultiPV. NNUE is far future.

## 10. Risks and open items

| Item | Mitigation |
|---|---|
| ~~fastchess passing option names with spaces (`option.Skill Level=0`)~~ | Resolved in Phase 0: fastchess splits each argument at its first `=`, and the smoke run confirmed that Skill 0 is applied. |
| Exact fastchess flags (`-openings`, `-pgnout`, adjudication) in v1.8.2-alpha | Check against `man.md` when building the xtask, and pin the fastchess version in `setup`. |
| ~~`UCI_Elo` calibration basis (CCRL scale, time control)~~ | Resolved in Phase 4: Stockfish 19's `search.h` says Skill 0..19 covers roughly CCRL Blitz 1320–3190, fitted to games against the Stash engine. Estimates at our faster time control are still approximate. |
| sfproxy rungs might not be evenly spaced | `xtask calibrate` measures them. Adjust *p* values if neighbouring rungs are too close. |
| Noise from the M4 Pro's mixed performance and efficiency cores | Default concurrency 8, below the performance-core count. Both engines in a game share the same conditions. |
| The Mac sleeping during a match (clocks keep running, so games are lost on time) | Run long matches under `caffeinate -i`. The faults checks catch it: calibrate refuses the result, and ladder blocks promotion. |
| A Stockfish upgrade changes the opponent | Use a snapshot binary in `tools/engines/`, and record the Stockfish version in every CSV row. |
| Draw and resign adjudication trusting buggy scores early on | Adjudication stays off until Phase 5 step 2. After that, resign adjudication is two-sided only. |

## 11. References

- Rustic book: https://rustic-chess.org. See the changelog, playing strength and
  SPRT-testing pages under `/progress/`, and the roadmap under `/introduction/`.
- chessprogramming.org: Getting Started, UCI, Perft Results, Engine Testing, SPRT, Match
  Statistics, Magic Bitboards, Looking for Magics, Simplified Evaluation Function,
  Encoding Moves.
- UCI specification: https://www.shredderchess.com/chess-features/uci-universal-chess-interface.html
- fastchess: https://github.com/Disservin/fastchess (`man.md` lists all options)
- Opening books: https://github.com/official-stockfish/books
