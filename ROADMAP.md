# Backrank roadmap

Plans and ideas for making Backrank stronger, in the order we intend to try them. This is a
living document: tick items off, record what happened, and add ideas as they come up.

[`docs/plan-v1.md`](docs/plan-v1.md) records how the engine got here and still defines v1, and
[`docs/plan-release.md`](docs/plan-release.md) covers branches and releases. This file takes
over from the list of later work in `plan-v1.md` §9.

**Last updated:** 2026-11-06, for v0.2.0 on the `sf-elo2000` rung.

## Contents

- [How to use this file](#how-to-use-this-file)
- [Where Backrank is now](#where-backrank-is-now)
- [The plan at a glance](#the-plan-at-a-glance)
- [Stage 0: Phase 6 and v1.0.0](#stage-0-phase-6-and-v100)
- [Stage 1: Better measurement](#stage-1-better-measurement)
- [Stage 2: Small search fixes](#stage-2-small-search-fixes)
- [Stage 3: Transposition table](#stage-3-transposition-table)
- [Stage 4: Tapered evaluation and endgames](#stage-4-tapered-evaluation-and-endgames)
- [Stage 5: Move ordering](#stage-5-move-ordering)
- [Stage 6: Principal variation search and aspiration windows](#stage-6-principal-variation-search-and-aspiration-windows)
- [Stage 7: Reductions and pruning](#stage-7-reductions-and-pruning)
- [Stage 8: Evaluation tuning and terms](#stage-8-evaluation-tuning-and-terms)
- [Stage 9: Advanced search and parameter tuning](#stage-9-advanced-search-and-parameter-tuning)
- [Stage 10: NNUE](#stage-10-nnue)
- [Stage 11: Multiple threads](#stage-11-multiple-threads)
- [Running alongside: performance](#running-alongside-performance)
- [Running alongside: UCI and features](#running-alongside-uci-and-features)
- [Running alongside: outside validation](#running-alongside-outside-validation)
- [Correctness backlog](#correctness-backlog)
- [Tried and rejected](#tried-and-rejected)
- [Ideas not yet scheduled](#ideas-not-yet-scheduled)
- [Decisions to make](#decisions-to-make)
- [Reference data](#reference-data)
- [Sources](#sources)

## How to use this file

- **Work in stage order, unless evidence says otherwise.** Each item has an ID (`TT3`, `P2`) to
  use in branch names, commits and `--note` text.
- **When an item is finished,** tick it and add its result on the same line, such as
  `H1 accepted, +54 [+30, +78], 626 games at 2+0.02`. Full rows live in `results/`.
- **When an experiment fails,** move it to [Tried and rejected](#tried-and-rejected) with its
  numbers and why it might be worth another look. A failed test is a result, and it stops us
  repeating it without a new reason.
- **New ideas** go in [Ideas not yet scheduled](#ideas-not-yet-scheduled) with one line on why.
  Promote them into a stage when there is evidence or a free slot.
- **Every search or eval change** follows the loop in `plan-v1.md` §8: tests, commit, SPRT,
  ladder, commit the `results/` rows, bump, pull request.

### How far to trust the numbers

Claims in this file come from three places, and they should not be treated alike:

- **Measured:** run on Backrank in this repository, on the development Mac (M4 Pro, 10
  performance cores, 4 efficiency cores, 24 GiB).
- **Sourced:** from another project, linked in [Sources](#sources). Elo figures from elsewhere
  show the rough relative size of a feature, not what Backrank will gain. They depend on the
  engine, its strength, the time control and the opponent. Self-play also flatters: Rustic's
  author found only about 60% of a self-play gain survives against other engines (`plan-v1.md`
  §3), and our draw detection's +38 didn't show against Stockfish at all.
- **Judgement:** reasoning, including the stage order, the rating targets and any starting
  parameters not attributed to a source. Change them freely when results disagree.

## Where Backrank is now

### What exists

| Area | Has | Doesn't have |
|---|---|---|
| Board and moves | Bitboards plus mailbox, magic sliders, Zobrist keys, make/unmake, pseudo-legal generation with a legality check in `make_move`, perft-verified | Legal or evasion generators, pawn key |
| Search | Iterative deepening, fail-soft negamax, quiescence (captures and queen promotions), check extension, partial iterations | Transposition table, PVS, aspiration windows, any pruning or reductions other than alpha-beta itself |
| Ordering | Previous PV move, MVV-LVA | Killers, history, SEE. **Quiet moves have no ordering at all** |
| Evaluation | Material and Michniewski's piece-square tables, with a sudden switch between two king tables | Tapering, tuning, pawn structure, mobility, king safety, any endgame knowledge |
| Time | `remaining/20 + inc/2` hard limit, half of that as the soft limit | Stability-based scaling, instant single replies |
| UCI | `Move Overhead`, all clock and limit types except `mate` and `searchmoves` | `Hash`, `Threads`, MultiPV, pondering |
| Tooling | Ladder, SPRT, calibration, perft-diff, self-play gate, release pipeline | `bench`, adjudication, a gauntlet of real engines, data generation, tuning |

### Baseline measurements

Taken 2026-11-06. Game figures come from the 240 games of the latest `sf-elo2000` ladder run
(commit `42848c3`). Search figures come from the six perft-suite positions at 500 ms each, using
a throwaway copy of the engine with counters added, so treat them as a small sample.

| Measure | Value |
|---|---|
| Ladder result | 58.5% vs `sf-elo2000`, Elo +60 [+17, +105], estimated rating 2060 |
| Search depth in games | Mean 6.7, median 6 |
| Stockfish's depth in the same games | Median 13, on a similar mean time per move (196 ms) |
| Time per move | Mean 176 ms |
| Clock left at the end of a game | Mean 22% of the 8 s base time, about 1.8 s |
| Nodes per second | 10 to 13 million |
| Perft speed | 68 million nodes per second |
| Growth per iteration | About 4.4 times (Kiwipete, depth 4 to 7) |
| Quiescence share of nodes | 78% to 92% |
| Quiescence nodes that stand pat at once | 61% to 85% |
| Cutoffs on the first move | 42% at the start position; 83% to 99% in the five sharper positions |
| Mean move number of a cutoff | 4.67 at the start position; 1.04 to 1.66 elsewhere |
| Quiet moves' share of cutoffs | 75% at the start position; 1% to 43% elsewhere |
| Illegal moves among those tried at interior nodes | 4% to 53% |
| Time forfeits by Backrank | 0 in the 26,866 ladder, SPRT and self-play games on disk |
| Self-play draw rate | 22% to 26% (last three SPRTs, 2+0.02) |

### What that shows

1. **Tree size is the main limit, not speed.** Backrank visits about 2 million nodes per move
   and reaches depth 6 or 7, while Stockfish reaches about 13. Around 90% of the nodes are
   quiescence nodes, so interior nodes run at only about 1 to 3 million per second. Stockfish's
   own data puts each extra ply in this range at 89 to 191 Elo. That figure comes from a far
   stronger engine, so read it as direction rather than a forecast.
2. **Quiet positions are searched in generation order.** With no killers or history, the
   start position's cutoffs arrive on move 4.67 on average, and only 42% on the first move.
   CPW says modern programs fail high on the first move about 90% of the time.
3. **Backrank loses by sudden collapse.** In 91 of 97 losses (94%), its own score fell by at
   least 2 pawns between consecutive moves, with a median worst drop of 6.55 pawns. In 71 of the
   97 it had thought itself at least a pawn ahead at some point. The worst drop comes a median
   82% of the way through the game. A drop can mean a tactic it couldn't see or a position it
   was misjudging, and these numbers don't separate the two ([M9](#stage-1-better-measurement)
   can).
4. **Endgames are a suspect, but the evidence is modest.** Positions with phase 8 or less (out of
   24) make up 57% of those collapses, or 50% in the previous `sf-elo2000` run. They are also 50%
   and 42% of its moves in lost games, so the excess is only about 7 or 8 points on about 90
   collapses each, and it's tangled up with collapses coming late in the game. Its losses do run
   long, at a median of 138 plies against 89 for wins.
5. **It struggles to convert.** In 4 of the run's 5 draws (and 8 of 14 in the previous run) it
   had scored at least +3.00 in the second half of the game. Most draws in both runs ended by the
   fifty-move rule. The samples are small.
6. **Hard limits cut iterations off.** A hard limit only twice the soft limit means an iteration
   started anywhere near the soft limit can't finish, when each iteration costs about 4.4 times the
   last. Keeping partial iterations gained +54 Elo, which suggests this waste was real. Games also
   end with about 1.8 s on the clock, or about 13% of what the base time and increments provide
   over an average game, though the time formula keeps some reserve by design.
7. **Plain adjudication isn't safe yet.** Replaying 2,677 self-play games, a resign rule got 8 of
   1,990 results wrong (0.4%), while a draw rule got 200 of 530 wrong (38%). See
   [M5](#stage-1-better-measurement).
8. **The 2060 estimate may be well too high.** Rustic Alpha 1, with nearly the same features, is
   rated 1675 on CCRL Blitz. A gap that size looks more like a difference of scale than of
   strength, and `plan-v1.md` already calls ours probably optimistic. Only playing real engines
   will settle it ([M6](#stage-1-better-measurement)).

These figures describe play against weakened Stockfish, which makes deliberate mistakes, so
they may not transfer exactly to other opponents.

## The plan at a glance

| Stage | Content | Evidence for its value | Ladder target | Comparable CCRL Blitz rating |
|---|---|---|---|---|
| 0 | Phase 6: `bench`, stress run, GUI checks, v1.0.0 | Needed for everything after it | `sf-elo2000` | Rustic Alpha 1: 1675; Leorik 1.0: 2112 |
| 1 | Better measurement | Lower noise, cross-checks | | |
| 2 | Small search fixes | Tens of Elo, cheap | `sf-elo2200` | |
| 3 | Transposition table | Rustic: +42 cutoffs, +103 ordering | | Rustic Alpha 2: 1815 |
| 4 | Tapered eval, endgame basics | Rustic: +248 with tuning; some support in our games | `sf-elo2400` | |
| 5 | Killers, history, SEE | Rustic: +56 for killers | | |
| 6 | PVS, aspiration windows | Rustic: +55 for PVS | | Rustic Alpha 3: 1865 |
| 7 | Reductions and pruning | Leorik 1.0 to 2.0: +425 (with a speed-up) | `sf-elo2600` | MinimalChess 0.6: 2443; Leorik 2.0: 2537 |
| 8 | Own data, tuning, eval terms | Texel: +99.6; Leorik 2.0 to 2.5: +384 (with other changes) | `sf-elo2800` | Leorik 2.5: 2921 |
| 9 | Singular extensions, correction history, SPSA | Stockfish 15: singular ~58 Elo | `sf-elo3000` | |
| 10 | NNUE on our own data | Carp's first net: +518 self-play; Leorik 3.0: about +300 of its +355 | `sf-elo3190` | Carp 2.0.0: 3159; Leorik 3.0: 3276 |
| 11 | Lazy SMP | Stockfish, 1 to 8 threads: +179 to +327 | Past the ladder | Carp 3.0.1: 3525 |

The targets and comparisons are rough and are judgement, not promises. Each comparison is another
engine's rating at roughly that feature set, listed in [Reference data](#reference-data), and they
aren't on one line: Rustic Alpha 3 has a TT, killers and PVS at 1865, while Leorik 1.0 reaches 2112
without pruning. Quality of implementation and evaluation matters as much as a list of features.
CCRL's scale and Stockfish's `UCI_Elo` scale also differ, and our estimates are probably optimistic
until the gauntlet in [M6](#stage-1-better-measurement) gives a second opinion.

### Why this order

- **Measure first.** Each stage after this one depends on trusting small differences.
- **Search before most evaluation.** Depth is the biggest gap, and every search feature below
  depends on the transposition table.
- **Tapered eval moves up from `plan-v1.md` §9.** There it came fifth. It is cheap, it had one of
  the largest gains in Rustic's history, and our games give some support for endgames being a weak
  spot, along with failures to convert.
- **Don't over-invest in handcrafted terms.** NNUE replaces them (Carp 2.0 and Leorik 3.0 both
  dropped their handcrafted eval completely). Tuning and the data generator carry over; individual
  terms mostly won't. Stage 8 should be time-boxed.
- **NNUE after the search is healthy.** Training data is only as good as the search producing it,
  and a slower evaluation needs an efficient tree to pay off.
- **Threads last.** They multiply what's there, and they make results less repeatable.

### Rules for experiments

- One change per SPRT, so a gain or loss can be attributed to something.
- Run depth-sensitive changes at 8+0.08, not 2+0.02. See [M3](#stage-1-better-measurement).
- Re-test old decisions after big changes. A feature rejected before the transposition table
  may pass after it, and a feature that helped alone may stop paying once others overlap with it.
- A pure speed-up has an identical `bench` node count and more nodes per second. That doesn't
  need a gainer SPRT.
- Backrank stays std-only, and `unsafe` needs a measured win and a `// SAFETY:` comment (see
  `CLAUDE.md`). Ideas from other engines are fine; copying their code is not. Reckless and
  Viridithas are AGPL-3.0 and Backrank is GPL-3.0-or-later.

## Stage 0: Phase 6 and v1.0.0

This is the plan's existing next step (`plan-v1.md` §5). Of the v1 definition of done in §1,
items 1, 3 and 4 are met. Items 2 (robust UCI) and 5 (tagged release) remain.

- [ ] **H1. `backrank bench`.** Search a fixed set of positions at a fixed depth and print total
  nodes and nodes per second. Use the format OpenBench expects, such as `4712710 nodes 1323423
  nps`, which costs nothing and keeps that door open. It must be deterministic: single thread, no
  time limits, state cleared between positions.
- [ ] **H2. Use the node count.** Fill the `bench_nodes` column already in `results/ladder.csv`
  (and add it to `sprt.csv`), put it in commit messages, and check that it matches on Linux, macOS
  and Windows in CI. `plan-release.md` §2 notes that the strength-evidence gate can then compare
  node counts: an unchanged count means no strength test is needed.
- [ ] **H3. Stress run.** 1,000 games mixing clock controls (including `1+0.01`) with fixed
  move-time games, requiring 0 crashes, 0 illegal moves, 0 protocol errors and under 0.5% time
  losses. `cargo xtask selfplay` covers clock games. Move-time games need a small extension
  (check fastchess's `st=` in its `man.md`). Run it under `caffeinate -i`.
- [ ] **H4. GUI checks.** Try Cute Chess or BanksiaGUI on macOS, and a GUI on Windows
  (`plan-release.md` §7), using release archives rather than local builds.
- [ ] **H5. Final climb and v1.0.0.** Run `cargo xtask ladder --climb`, update the README's rating,
  then `cargo xtask bump major`.
- [ ] **H6. Finish the rename.** `repository` in `Cargo.toml` still has the old name and should
  change with the next bump. The next release is also the first signed under the new name, so
  check that `gh attestation verify` in `.github/release-footer.md` passes on it.

## Stage 1: Better measurement

These can be done alongside Stage 0 or 2. M1 and M6 matter most.

- [ ] **M1. Search statistics.** Add a `stats` cargo feature (no cost when off, no dependency)
  and report: quiescence share, first-move cutoff rate, mean cutoff move number, quiet share of
  cutoffs, illegal moves tried, and later TT hit rate and hashfull. Print them from `bench`. The
  baseline table above came from a throwaway version of this.
- [ ] **M2. Document SPRT bounds.** `cargo xtask sprt` already takes `--elo0` and `--elo1`. CPW's
  table puts us in "all other engines": gainers `[0, 10]`, non-regression `[-10, 0]`. Move to
  `[0, 5]` and `[-5, 0]` ("top 200") when gains get small, and `[0, 3]` and `[-3, 1]` ("top 30")
  much later. Halving the bound takes roughly four times the games, so tighten only when needed.
- [ ] **M3. Settle the time controls.** At about 1,200 games an hour, 8+0.08 is the right default,
  and CPW's example and Viridithas both use it. The last three SPRTs used 2+0.02 for speed, which
  risks favouring anything cheap and penalising anything that pays off with depth. Add a long test
  (40+0.4, about 240 games an hour) for features whose value depends on depth, like LMR and
  singular extensions. Stockfish uses 10+0.1 and 60+0.6.
- [ ] **M4. Opening book plan.** We use `noob_3moves`. CPW recommends balanced books such as
  `8moves_v3.pgn` for weaker engines and `UHO_Lichess_4852_v1.epd` for stronger ones, because a
  biased book lowers the draw rate. Move to `8moves_v3` when self-play draws pass about 50%, and
  UHO when they pass about 70% (thresholds are judgement). `ladder.csv` doesn't record the book,
  so add that column before changing it, or the history stops being comparable.
- [ ] **M5. Resign adjudication for SPRTs.** Replaying 2,677 finished self-play games at 2+0.02,
  with a script approximating fastchess's rules:

  | Rule (fastchess's example values) | Time saved | Results wrong |
  |---|---|---|
  | `-resign movecount=3 score=600 twosided=true` | 7% | 8 of 1,990 |
  | Adding `-draw movenumber=34 movecount=8 score=20` | 11% in total | 200 of 530 adjudicated draws |

  Turn on resign only, and only in self-play: never in ladder games, where the opponent makes
  deliberate errors. Leave draw adjudication off until scores near zero mean something (probably
  after Stage 8), then repeat this replay. OpenBench advises adjudicating "aggressively, so long
  as the accuracy of adjudication remains very high". This one is about 7% of test time, so it's
  nice but not urgent.
- [ ] **M6. A gauntlet of real engines.** Add `tools/gauntlet.toml` and `cargo xtask gauntlet`
  using fastchess's `-tournament gauntlet`, with results in `results/gauntlet.csv`. Weakened
  Stockfish searches deep and then deliberately picks a worse move, so its errors don't look like
  a real engine's. Our 2060 sits oddly against Rustic Alpha 1's 1675 with similar features. Past
  `3190` the ladder runs out too. This is the most important item in this stage, because it could
  move every target in this file. Candidates, all open source with pinned tags:

  | Opponent | CCRL Blitz | Notes |
  |---|---|---|
  | Rustic Alpha 1, 2, 3 | 1675, 1815, 1865 | Rust. Tag `alpha-3.0.0` on Codeberg is the CCRL version |
  | Reckless v0.1.0 | 2005 | Rust. Tags confirmed for every version here |
  | Reckless v0.2.0 | 2349 | |
  | Reckless v0.3.0 | 2616 | |
  | Reckless v0.4.0 | 2934 | |
  | Carp `v.2.0.0` (tag has a stray dot) | 3159 | Rust. First NNUE version |
  | Reckless v0.5.0 | 3240 | |

  Ratings come from each project's README and drift over time. Whether old tags still build on
  Apple Silicon with a current toolchain is untested, and playing against AGPL engines is fine
  since nothing of theirs ships.
- [ ] **M7. Tablebase adjudication in tests.** fastchess can adjudicate with Syzygy files
  (`-tb`, `-tbadjudicate`) with no engine support. The 3-to-5-piece set is 939 MiB. OpenBench calls
  this useful for saving time.
- [ ] **M8. Regression gate in CI.** Add a mate and draw suite for correctness (extending
  `tests/search.rs`), not strength: CPW warns test suites correlate poorly with playing strength.
  Pruning will break tests that assert exact depths, so write them with time or generous depth.
- [ ] **M9. `cargo xtask analyse`.** Turn this file's scratch analysis into a repeatable report:
  depth, clock use, collapses by phase, and conversion failures. Then go further: send each
  collapse position to Stockfish at fixed depth and classify it as a missed tactic (a reasonable
  depth finds it) or a misjudged position. That tells us which of search and eval to work on next,
  and which eval terms matter. fastchess can write `notation=uci`, which `backrank`'s own library
  can replay without a SAN parser.
- [ ] **M10. Removal tests.** After Stage 7, test taking each feature out. Features that no longer
  pay should go.

## Stage 2: Small search fixes

Cheap and separate from the larger features. Each is its own SPRT, unless noted.

- [ ] **T1. Move at once with one legal reply.** On a clock, only one legal move means no reason
  to search (CPW). Still send a scored `info` line first, because `fastchess --compliance`
  requires one. Don't apply it with `go depth`, `go nodes`, `go movetime` or `go infinite`.
- [ ] **T2. Stop on a forced mate.** On a clock, stop deepening once a mate score is shorter than
  the depth just completed. There is no pruning yet to make the score unreliable, but revisit
  this at Stage 7. Reckless handles "forgotten mates" explicitly in its root loop.
- [ ] **T3. Widen the hard limit.** Today hard is `remaining/20 + inc/2`, and soft is half of it.
  Test a hard limit several times the soft limit, keeping soft about where it is. For comparison,
  Reckless uses soft 1.2% to 5.9% of remaining time (rising with move number) and hard 72.8%, each
  plus 0.75 of the increment. Those values are tuned for that engine, and CPW says plain
  `base/20 + inc/2` is "very competitive", so test rather than assume. Re-run the stress test.
  Worth also testing `inc * 0.75`.
- [ ] **T4. Scale the soft limit by stability.** Spend less when the best move and score are
  stable, and more when they change or drop (CPW; Reckless tracks `eval_stability` and
  `pv_stability`). Noisy until the TT arrives, so do this late in Stage 3 or after Stage 6.
- [ ] **T5. Twofold in the tree, threefold before it.** `Position::is_repetition` scores any
  earlier occurrence as a draw, including one from before the root. Stockfish scores a position
  as a draw only if it repeats once strictly after the root, or twice before or at it. Otherwise a
  move returning to a position seen once in the game looks like a draw when it isn't. The test
  `the_losing_side_takes_a_repetition` expects the current behaviour and would need to change.
- [ ] **T6. Mate distance pruning.** Safe and simple: bring alpha and beta inside the best mate
  still possible at this ply (CPW has the code). It adds little Elo but finds mates faster, with
  Glass reporting 7.8 million nodes falling to 3.6 million in one example. Not at the root.
- [ ] **T7. Delta pruning in quiescence.** With quiescence at 78% to 92% of nodes, skip captures
  that can't raise alpha even with a margin (`stand_pat + victim + margin < alpha`), with extra
  care for promotions. Start with a margin of about 200. SEE pruning (`O4`) goes further later.

## Stage 3: Transposition table

Rustic gained +42 for cutoffs and +103 for ordering (`plan-v1.md` §3). In Stockfish, 75% of
cutoffs in a position with a hash move come from the hash move (CPW). Nearly everything after this
stage depends on it.

- [ ] **TT1. `Position::is_pseudo_legal(m)`.** A TT move may come from a colliding position, and
  `make_move` panics or corrupts the board if given one (see its doc comment). Test it as a
  property: at every node of a perft walk, for moves collected from other positions,
  `is_pseudo_legal(m)` must equal "the generator produces `m` here". Don't skip this: the test is
  cheap and catches the worst class of TT bug.
- [ ] **TT2. The table.** Design it for threads from the start, as this needs no `unsafe` and
  costs nothing now:
  - `Box<[AtomicU64]>`, two words per entry, `Relaxed` loads and stores. Rust's memory model makes
    this race-free, which raw shared memory is not.
  - Word 1 packs move (16 bits), score (16), static eval (16), depth (8), bound (2) and age (6).
    Word 0 is `key ^ word1`, Hyatt's lockless check: a torn entry fails to match. `Relaxed` gives
    no ordering between the two words, and the XOR is what makes that safe.
  - Index with `((key as u128 * entries as u128) >> 64)`, so any size works and the top key bits
    choose the slot.
  - Begin with one entry per slot, always replacing. Then try buckets with depth and age
    preference (`TT8`).
- [ ] **TT3. `Hash` option.** Spin, default 16 MiB. Resize only between searches. Clear on
  `ucinewgame` and offer `Clear Hash`. The table has to outlive each `go`, so `Engine` owns an
  `Arc` that goes to the search thread, and `search()` gains a parameter. Report `hashfull`.
  Clearing or allocating a large table mustn't hold up `isready` for long.
- [ ] **TT4. Hash move ordering.** At interior nodes the hash move takes the `pv_move` slot in
  `Ordered::new`, and `prev_pv`/`on_pv` can go. Validate it with `TT1`. **Keep the previous best
  move explicit at the root:** `search_root`'s partial-iteration rule depends on it being tried
  first, and its TT entry can be overwritten during the search. The PV table stays for reporting.
  SPRT this alone, as it should be the larger half of the gain.
- [ ] **TT5. Cutoffs.** Entry depth at least the current depth, bound compatible with the window,
  not at the root and not in PV nodes (CPW says advanced engines avoid them there, which also keeps
  PV lines complete). Check draws first. Skip when the halfmove clock is high (Stockfish 15 uses
  `rule50_count() < 90`), since scores don't account for the fifty-move rule.
- [ ] **TT6. Mate scores.** Scores in the search count from the root, but an entry can be reached
  at a different ply. Store as distance from the node (add `ply` to a winning mate score, subtract
  for a losing one) and undo that on loading. Getting this wrong produces confident wrong mates.
  Test with mates that transpose.
- [ ] **TT7. Use stored eval and score.** Reuse the stored static eval, and let a compatible
  stored score replace it (Stockfish 15: ~4 Elo in the main search, ~7 in quiescence). Small
  now, and it matters more with NNUE, where eval calls are expensive.
- [ ] **TT8. Replacement and sizing.** Try buckets with age. Leorik and MinimalChess both use two
  entries with aging, and for MinimalChess that was part of a 0.6 release its author puts at about
  200 Elo. Reckless packs 3 entries into a 32-byte cluster. Stockfish says keeping average hashfull below 30% is best, and
  its tables show little loss until then, then about 10 Elo or more per further halving.
- [ ] **TT9. Pick the test hash.** At 176 ms per move, interior nodes make about 200,000 to 500,000
  stores, and adding quiescence nodes would be around 2 million. A 16 MiB table is about a million
  16-byte entries, so storing quiescence nodes could fill it every move. Test storing them, then
  test 16 vs 64 MiB, and record the choice.
- [ ] **TT10. Probe in quiescence.** Separate SPRT, because it changes quiescence's cost balance.
- [ ] **TT11. Re-test the rejected quiescence idea.** See
  [Tried and rejected](#tried-and-rejected).

**Watch out for:**

- **Repetition:** a score reached along one path gets reused on another where a repetition differs.
  That's the graph history interaction problem, which all engines accept. Don't try to solve it.
- **Reported lines:** the existing test that every reported line is legal should catch TT problems
  early. Keep it passing.
- **Determinism:** `bench` must clear the table between positions.

## Stage 4: Tapered evaluation and endgames

Rustic gained +248 in self-play with tapered and tuned eval. Pawel Koziol reported about 200 Elo
giving TSCP the PeSTO tables (second-hand). MinimalChess went from 1439 to 1816 with tapered
tables, killers and staged generation. It's also a cheap test of whether our failures to convert and
the modest endgame signal come from evaluation (see points 4 and 5 under
[What that shows](#what-that-shows)).

- [ ] **E1. Tapered tables.** Keep middlegame and endgame totals, blended by phase:
  `(mg * phase + eg * (24 - phase)) / 24`, where phase is N 1, B 1, R 2, Q 4, capped at 24 for
  promotions. This removes `is_endgame` and the sudden jump in the king table when a piece comes
  off. Begin with PeSTO's published tables (material mg 82, 337, 365, 477, 1025 and eg 94, 281,
  297, 512, 936). Replace them with our own in Stage 8, as Viridithas did with its first version.
- [ ] **E2. Incremental evaluation.** Update `mg`, `eg` and phase in `put_piece` and
  `remove_piece`, and extend `check_invariants` to compare them with a recount, as the key does.
  `evaluate` loops over every piece at every quiescence node today, so this should be an easy speed
  win. Worth doing in the same pull request.
- [ ] **E3. Mating with bare kings.** Add a term for positions with no enemy pawns or pieces:
  push the enemy king to the edge and bring ours closer. Test that KQK, KRK and KBBK are mated
  from a few positions within the fifty-move limit at 100 ms a move. KBNK is the stretch case.
- [ ] **E4. Fifty-move awareness.** Scale the score towards zero as the halfmove clock rises, so
  the engine makes progress instead of drifting into a fifty-move draw. Most of our ladder draws
  end by that rule. Worth a try with E3.
- [ ] **E5. Draw-ish material.** Scale down scores in endings that are hard to win: KNN vs K,
  minor piece up without pawns, opposite-coloured bishops. Tiny effort, check with M9 first.

## Stage 5: Move ordering

Rustic: +56 for killers. Target: first-move cutoffs near 90% across a broad position set, from a
baseline of 42% at the start position. Interior search gets cheaper without any change in what's
searched, so this also multiplies the value of later reductions.

- [ ] **O1. Killers.** Two quiet moves per ply that caused a cutoff, tried right after winning
  captures. Only reordering generated moves for now, so no legality check is needed.
- [ ] **O2. History heuristic.** `[colour][from][to]`, with bonus on the cutoff move and malus on
  quiets tried before it, using the gravity update from CPW so values stay bounded:
  `h += bonus - h * |bonus| / MAX`. Start with `bonus = min(300 * depth - 250, cap)`. Decide
  whether to keep it between moves in a game by test. It also feeds LMR later, where Stockfish 15
  attributes ~30 Elo to history alone.
- [ ] **O3. Static exchange evaluation.** A threshold form (`see_ge(move, threshold)`) is enough
  and cheaper than a full value. Needs attackers-to-square with x-rays. Test against known
  exchanges in unit tests, including en passant and promotions. Leorik's 2.4 release, adding SEE to
  quiescence and search, went from 2677 to 2831 on Blitz and from 2736 to 2799 on 40/15, though it
  also changed null move and futility pruning and added some draw recognition.
- [ ] **O4. Use SEE.** Split captures into good and bad, trying bad ones after quiets, and skip
  negative-SEE captures in quiescence. Stockfish 15 credits ~5 Elo for the last of these, though
  in our 90%-quiescence tree it should be worth much more.
- [ ] **O5. Countermove.** A quiet move indexed by the previous move, which tends to complement
  killers.
- [ ] **O6. Capture history.** Replaces the LVA part of MVV-LVA with learned values. Carp orders
  captures with MVV plus capture history, using SEE as a threshold.
- [ ] **O7. Continuation history.** History keyed by the last one or two moves. Standard in strong
  engines, and needs a move stack in the search.
- [ ] **O8. Staged generation.** Hash move, then captures, then killers, then quiets, generating
  only as needed. This is mostly a speed feature, MinimalChess and Leorik both have it, and it
  needs `is_pseudo_legal` for killers and countermoves.

## Stage 6: Principal variation search and aspiration windows

Rustic: +55 for PVS. CPW expects about 10% less search effort.

- [ ] **W1. PVS.** Full window on the first move at PV nodes, null window on the rest, and re-search
  when a move lands inside the window.
- [ ] **W2. Aspiration windows.** Around the last iteration's score, about 25 to 50 centipawns each
  side (CPW), starting around depth 4 or 5. On a fail, widen only the failing bound, exponentially.
  Open the window fully near mate scores.
- [ ] **W3. Revisit partial iterations.** This needs care. `search_root` currently accepts a
  stopped iteration because "any move that then beat it has an exact, better score". With PVS and
  aspiration windows that stops being true:
  - A null-window fail high is only a bound until its re-search finishes.
  - An aspiration fail low at the root gives no trustworthy best move (CPW warns of this).
  - The rule also needs the previous best move to be searched first (see `TT4`).

  So a stopped search may only adopt a move whose full-window search finished. Add a test that
  stops the search at many node counts and checks the move is one the iteration really proved.
  `stopping_at_any_point_gives_a_consistent_result` is a start.
- [ ] **W4. Spend more on root fail lows.** A failing best move is when extra time is most useful
  (CPW's "panic time"). Goes with T4.

## Stage 7: Reductions and pruning

The biggest step in the classical part of the plan. **Leorik went from 2112 to 2537 Blitz** with the
same piece-square tables by adding null move, futility, history-sorted quiets and reduced late
quiets, along with a speed increase. Its author says that let it "look twice as deep". MinimalChess's
author puts its 0.6 release, adding LMR, deep futility and history-sorted quiets, at about 200 Elo.
Stockfish 15's annotations put LMR at ~98 Elo and shallow pruning at ~98.

**Before starting:**

- `depth` in `negamax` is `u32`, and `depth - 1` assumes it's positive. Reductions need a signed
  depth, or careful saturation. Change it once, in its own commit.
- Add `eval` and an `improving` flag (is the static eval better than two plies ago?) to a per-ply
  search stack. Many conditions use them.
- Expect `tests/search.rs` to need more than the current fixed depths for mates once pruning
  arrives (`M8`).

Add these one at a time, in this order. Parameter values are starting points from the cited
sources, and SPSA can refine them later (`X7`).

- [ ] **P1. Null move pruning.** If passing still fails high, this node isn't worth searching.
  - Conditions: not PV, not in check, static eval at least beta, side to move has a non-pawn piece
    (zugzwang), previous move wasn't null.
  - Reduction: start with `R = 3 + depth / 3`, then add a term for `(eval - beta) / 200` capped at
    about 3. CPW gives 3 or 4 as the basic fixed value. Stockfish 15 uses
    `min((eval - beta) / 147, 5) + depth / 3 + 4`, tuned for its own engine.
  - Don't return a mate score from it.
  - Needs `Position::make_null_move`. **Backrank-specific:** `is_repetition` looks back through
    `history` by `halfmove_clock`, and must not see through a null move. Set the halfmove clock to
    0 in the null move, restored by its `Undo` entry. Clear en passant, and update the key. Test
    this in the make/unmake property test.
- [ ] **P2. Late move reductions.** Search late quiets at lower depth, then re-search if they beat
  alpha. CPW's options: Ethereal `0.7844 + ln(depth) * ln(moves) / 2.4696` for quiets; Weiss
  `1.35 + ln(depth) * ln(moves) / 2.75` for quiets and `0.20 + ln(depth) * ln(moves) / 3.35` for
  captures. Start with depth at least 3, after the first few moves, not in check, not for
  killers. Reduce less in PV nodes, more at expected cut nodes, and adjust by history once `O2` is
  in. Clamp so a reduced search stays at depth 1 or more. Precompute the table at startup, since
  `ln` isn't a `const fn`. `attacks::init` shows the pattern.
- [ ] **P3. Reverse futility pruning.** If `eval - margin >= beta` at low depth, return. Margin
  about `150 * depth` for depth under 8, reduced when improving (CPW, Stockfish 15's child-node
  futility: `168 * (depth - improving)`, ~25 Elo). Not in PV nodes or in check. Skip near mate
  scores.
- [ ] **P4. Futility pruning.** Skip quiet moves at low depth if static eval plus a margin can't
  reach alpha. Stockfish 15: `eval + 122 + 138 * lmrDepth + history / 60 <= alpha`, ~9 Elo. Keep
  checks and captures, and make sure a node with all moves pruned isn't scored as stalemate.
- [ ] **P5. Late move pruning.** Stop trying quiets after a depth-based count. Stockfish 15:
  `moveCount >= (3 + depth * depth) / (2 - improving)`, ~7 Elo.
- [ ] **P6. SEE pruning in the main search.** Skip moves losing too much by SEE at low depth.
  Stockfish 15: `!see_ge(move, -203 * depth)` for captures and checks, ~9 Elo. Needs `O3`.
- [ ] **P7. Internal iterative reductions.** With no hash move, reduce depth at PV and expected cut
  nodes. Start by reducing by one above about depth 5 (CPW's example). Stockfish 15 reduces by 2 at
  PV nodes from depth 3, and by 1 at cut nodes from depth 8, for ~3 Elo.
- [ ] **P8. History pruning.** Skip quiets with very bad history at low depth (Stockfish 15: ~2
  Elo; Reckless and Carp have it).
- [ ] **P9. Razoring.** At low depth with eval far below alpha, verify with quiescence and return.
  Stockfish 15: `eval < alpha - 348 - 258 * depth * depth`; a later comment says ~1 Elo. CPW calls
  the classic form risky. Low priority, but cheap to try.

**Guards for every pruning rule:** not at the root, not when already mated or near mate scores
(`MATE_BOUND`), and not when in check unless a rule says so. Stockfish notes that "depth conditions
are important for mate finding".

## Stage 8: Evaluation tuning and terms

Texel's author reports +99.6 Elo in steps from tuning. Leorik gained 384 on Blitz between 2.0 and
2.5 through pawn structure, mobility, tuning from scratch on its own games and SEE, though Elo
doesn't all come from eval there. **Time-box this stage.** Tuning infrastructure carries into NNUE,
and individual terms mostly don't.

### Data

- [ ] **D1. `cargo xtask datagen`.** Self-play to labelled positions. It belongs in `xtask`, since
  `sfproxy` already links the `backrank` library and the engine stays dependency-free. Requires a
  soft node limit in `Limits` (stop between iterations once reached), as `nodes` today is hard.
  Settings from strong engines' source:

  | Setting | Carp | Viridithas |
  |---|---|---|
  | Search per move | Depth 8 | 25,000 nodes (soft limit) |
  | Random opening plies | 8 or 9 | 8 or 9 from the start position |
  | Reject unbalanced openings | Depth 10 search, `abs(eval) >= 1000` | 50,000 node search, `abs(eval) > 1000` |
  | Positions kept | Not in check, not a mate score, best move quiet, past ply 16 | Filtered when training |
  | Win adjudication | 4 moves at ±2000 cp | 4 moves at ±2500 cp |
  | Draw adjudication | 12 moves at ±5 cp | 12 moves at ±4 cp |

  Only use own games: Viridithas and Carp both do, and it keeps Backrank's strength its own. Note
  that a draw rule this tight needs an eval it can trust, so tune against our results (`M5`).
- [ ] **D2. Format.** Text is fine at first: bullet takes `<FEN> | <score> | <result>`, with score
  and result from white's point of view (1.0, 0.5 or 0.0). Move to a compact binary format once
  sizes hurt. Write the first file in the format the eventual trainer wants, to avoid repeating work.

### Tuning

- [ ] **D3. `cargo xtask tune`.** Minimise the error between game results and
  `sigmoid(K * score)`, after fitting `K` once. Use gradient descent over a linear evaluation model
  (as `texel-tuner`, based on Ethereal's paper, does) rather than the original ±1 search, which
  takes hours. Forum reports: one post describes plain gradient descent converging in under 10,000
  iterations, and another describes batch gradient descent with Adam taking about 130 for 10 million
  positions and 750 parameters. Dataset sizes that worked: Texel's own 8.8 million positions from 64,000 fast games;
  MinimalChess's 725,000 quiet positions. Rust crates are allowed here.
- [ ] **D4. Tune E1's tables** on our own data, replacing PeSTO's. Viridithas gained strength
  doing the same.

**Pitfalls, from forum reports and Texel's page:**

- Use quiet positions, or run quiescence during tuning. Tactical positions cause erratic values.
- Check the point of view of labels against the eval's, a classic bug.
- Don't overrate tuned values: "correlation does not imply causation", and a tuner can't teach what
  the evaluation has no term for. In Texel's list the largest single gain (39.4) came from changing
  which positions were kept.

### Terms

Add one at a time with tuning. Order by what `M9` shows, and this is the starting guess:

- [ ] **E6. Passed pawns.** A bonus rising as it advances, adjusted by king distance and blockers.
  First because of the long losses and failures to convert, if `M9` agrees.
- [ ] **E7. King activity in endgames** comes from the tuned tables. Check before adding anything.
- [ ] **E8. Mobility.** Leorik gained 124 on Blitz in 2.2, a release that also rewrote its TT
  replacement and time control. Per piece, by phase.
- [ ] **E9. Pawn structure.** Doubled, isolated, backward, connected. Leorik gained 29 on Blitz and
  69 on 40/15 in 2.1. A pawn hash helps, with hit rates above 95% (CPW), and needs a pawn key in
  `Position`, which correction history wants anyway.
- [ ] **E10. King safety.** CPW calls it "one of the most challenging tasks... but also the most
  rewarding". Attack units through a table (minor 2, rook 3, queen 5), plus pawn shield.
- [ ] **E11. Smaller terms.** Bishop pair, rook on open or semi-open file, rook on the seventh,
  outposts, tempo. Cheap to add once a tuner exists.

## Stage 9: Advanced search and parameter tuning

Top-engine techniques that need a healthy TT and history first. Values are Stockfish 15's.

- [ ] **X1. Singular extensions.** If a reduced, null-window search excluding the hash move fails
  below `ttScore - margin`, extend the hash move. Needs an `excluded` move in the search. Only when
  the entry has enough depth with a lower or exact bound. ~58 Elo, the biggest of the extension
  family.
- [ ] **X2. Multi-cut and negative extensions.** Follow-ups to `X1` when the exclusion search shows
  other moves are also good (Reckless has both).
- [ ] **X3. Correction history.** Record `search score - static eval` per pawn key, then adjust
  future evals by it. Only update when not in check, the best move is absent or quiet, and the
  bound doesn't contradict the eval (CPW). First seen in Caissa in 2023, adopted widely, and heavily
  used in Stockfish 18. Leorik added it in 3.1. Add other keys later (material, non-pawn, last
  move). Works with both handcrafted and NNUE evals, so it needs no waiting.
- [ ] **X4. History in LMR.** Reduce less with good history (~30 Elo). If `P2` didn't include it.
- [ ] **X5. ProbCut.** Good captures failing high by a margin at reduced depth cut the node. ~4 Elo.
- [ ] **X6. Revisit the check extension.** It gained +129 here when it was the only extension.
  Stockfish 15 puts it at ~1 Elo. Test narrower versions after `P2`.
- [ ] **X7. SPSA tuning.** Expose search constants as UCI options behind a `tune` feature (Reckless
  has a `spsa` feature) and run [weather-factory](https://github.com/jnlt3/weather-factory) with
  fastchess. OpenBench's defaults: alpha 0.602, gamma 0.101, A about 10% of iterations, `R_end`
  0.002, `C_end` about 1/20 of a parameter's range, 8 game pairs per point. Viridithas credits this
  with about 10 Elo, so run it after features settle, not before.
- [ ] **X8. TT-PV flag.** Remembering that a position was on the PV reduces and prunes it less.
  Stockfish 15 credits ~3 Elo, and Reckless stores it in the TT.
- [ ] **X9. Later ideas seen in Reckless's search:** hindsight reductions, low-depth singular
  extensions, bad-capture futility pruning, pawn history, continuation correction history. Their
  value here is unknown.

## Stage 10: NNUE

NNUE is what separates a strong engine from a top one. Carp's first network scored
**+518 ± 208** against its handcrafted eval, on 100 million positions. Leorik jumped 355 on Blitz
with a small network on 622 million positions, crediting about 50 of that to aspiration windows and
staged generation. Both replaced their handcrafted eval completely.

### Why this is practical here

- **Hardware is adequate.** bullet, the most widely used trainer as of 2025 (CPW), has a `metal`
  backend for macOS. Training speed on this Mac is untested, so check it first (`N1`).
- **Data is achievable.** At 25,000 nodes a move and about 12 million nodes per second, one core
  makes about 500 positions a second, or 100 million overnight across 10 cores. This is a rough
  sum, ignoring filters and NNUE's own slowdown.
- **No external crates are needed in the engine.** Weights go in with `include_bytes!`, and SIMD
  comes from `std::arch`. The trainer lives outside the shipped crate.

### Steps

- [ ] **N1. Spike the pipeline.** Run a very small dataset all the way through: `datagen`, bullet
  on Metal, a saved network, loading it in a scratch branch. Time one superbatch. Do this before
  investing further, as it finds every format problem cheaply.
- [ ] **N2. Generate data.** Target at least 100 million filtered positions (`D1`), shuffled.
- [ ] **N3. Train a first network.** bullet's `examples/simple.rs` is the template: `(768 -> N)x2 ->
  1` with dual perspective, SCReLU, `SCALE` 400, `QA` 255, `QB` 64, AdamW, batch size 16,384. Use
  `N` of 128 or 256. Carp's first net used 256, learning rate `1e-2`, 80 epochs with a drop at 30,
  and WDL 0.1. bullet's example uses WDL 0.75, so this is worth exploring. Keep the trainer in its
  own Cargo project outside the workspace, so GPU dependencies don't reach `Cargo.lock` or CI.
- [ ] **N4. Inference.**
  - One accumulator per perspective. Features are `colour * 384 + kind * 64 + square`, relative to
    each perspective. Match bullet's `Chess768` layout exactly.
  - Keep accumulators on a per-ply stack in `Searcher`, not in `Position`, so perft stays fast. Make
    `make_move` report which pieces changed, and copy-then-update from the parent's accumulator.
  - Output with SCReLU, as in bullet's `simple.rs`: divide the layer's sum by `QA`, add the bias,
    multiply by `SCALE`, then divide by `QA * QB`. Plain CReLU has no first division.
  - **Test:** after any line of moves, the incrementally updated accumulator equals a fresh build,
    in the spirit of `check_invariants`. Compare outputs with the trainer's for a few positions.
    Viridithas once "mis-implemented the memory layout of the subnet weights".
  - Parse weights at startup into aligned arrays. `include_bytes!` gives alignment 1, so don't
    reinterpret the bytes in place. Use a `OnceLock`, like the attack tables.
- [ ] **N5. SIMD.** Start with fixed-size loops over `i16`, which LLVM vectorises for add and
  subtract, and measure. Only then use `std::arch` for the output layer:
  - **ARM:** NEON is baseline and stable, so the Mac needs no dispatch at all.
  - **x86-64:** builds use `x86-64-v2` (`plan-release.md` §3), so no AVX2. Either add a `v3` asset,
    or dispatch at runtime as Stockfish 19 does. Safe `#[target_feature]` functions (Rust 1.86) and
    safe intrinsics inside them (1.87) reduce this to a single `unsafe` call after
    `is_x86_feature_detected!`. Reckless instead ships separate binaries, selected at compile time.
  - All versions must give identical results, or `bench` stops matching across machines.
  - Note the x86-64 `v3` finding in `plan-release.md` §3 (only about 2% more, not worth a second
    download) was for today's evaluation. NNUE changes that.
- [ ] **N6. SPRT against the handcrafted eval,** then ladder and gauntlet. Check the speed cost:
  CPW says Stockfish's NNUE stayed stronger with about half the search speed.
- [ ] **N7. Remove the handcrafted eval,** keeping piece values for SEE and ordering. The gates
  already treat `src/eval/**` as needing strength evidence.
- [ ] **N8. Generation loop.** Generate new data with the new net, train, test, repeat. Viridithas
  gained +20, +50, +20, +25 in its early nets, with occasional failures.
- [ ] **N9. Widen as data grows.** Carp's history makes the relationship clear: 384 nodes lost at
  100 million positions and won at 230 million; 512 lost at 230 million and won at 670 million.
  Carp 3.0.1 reached 3525 with `(768 -> 1024)x2 -> 1` and no king buckets, so **simple nets go a long
  way**.
- [ ] **N10. Output buckets.** Choose the output weights by material count, as in bullet's
  `progression/2_output_buckets.rs`. Leorik uses 8.
- [ ] **N11. Input buckets.** Separate weights by king position, with horizontal mirroring
  (`progression/3_input_buckets.rs`; Leorik uses 5). A king move across a bucket means a full refresh,
  so add a Finny table to cache accumulators per bucket. A forum post suggests at least a billion
  positions.
- [ ] **N12. Deeper networks.** `progression/4_multi_layer.rs`, with much care for speed.
- [ ] **N13. Threat inputs.** Far off. Stockfish 18 added threat features (SFNNv10), Stockfish 19
  pruned them and added pawn-pair features (SFNNv16), and Reckless has threat accumulators.

### Training pitfalls others hit

- **Unshuffled data.** Viridithas's first net was "much weaker than the HCE". The same data,
  shuffled, "crushes HCE". A later entry in its history reads "whoops! forgot to shuffle".
- **Mislabelled scores.** Viridithas had to redo 320K games that had been scored with the
  handcrafted eval.
- **Too little data for the size.** See `N9`. After a net on its own data alone lost about 60 Elo,
  Viridithas's author wondered whether "960k games is not enough to train a good net".
- **Overfitting.** Carp's 670 million position run with AdamW lost 19 Elo: "Turns out overfitting is
  a thing". More data with the same network isn't automatically better.
- **Noisy positions.** Viridithas credits filtering them, along with more data, with about 20 Elo.
- **Lost data.** Keep generated data backed up, and name runs by what made them. Viridithas
  "accidentally deleted half of it". Don't commit training data.

### Networks in the repository

Weights take `768 * N * 2` bytes, or about 200 KB at `N = 128` and 1.6 MB at `N = 1024`. Plain
`git` copes with that for a while. Keep networks under `src/eval/`, because `check-release` treats
`src/` as shipped and so demands a version bump and evidence. Note how each was made in a text file
next to it, as Carp and Viridithas both do, since this history is valuable later.

## Stage 11: Multiple threads

- [ ] **L1. Lazy SMP.** Threads search the same root and share only the TT (`TT2` already makes it
  safe), diverging through their own history and through varied depth. Use `std::thread::scope`,
  with a per-thread clone of `Position`. Add a `Threads` option. Each thread owns its search stack
  and history.
- [ ] **L2. Pick the result.** Thread voting, weighted by depth and score, or just the main thread's
  move. CPW gives Berserk's and PlentyChess's rules.
- [ ] **L3. Aggregate reporting.** Node counts and `hashfull` across threads, and the stop flag.
  `go nodes` needs a decision about aggregate versus per-thread limits.
- [ ] **L4. Testing.** Run 4 threads against 1 at 8+0.08, with fewer concurrent games. Expect noise,
  because Lazy SMP isn't deterministic. `bench` stays single-threaded.

Stockfish's measurements: 8 threads vs 1 gives +178.6 ±14 at 60+0.6, and +327 ±15 at 10+0.1, about
82% as good as giving one thread eight times the time. Those are Stockfish's numbers, so measure our
own. CCRL's lists show entries by CPU count, and a good single thread remains the priority.

## Running alongside: performance

Do these when profiles point to them, starting after Stage 3. `cargo build --profile profiling`
already gives symbols. Stockfish's rule of thumb is 1% speed ≈ 2.1 Elo at 10+0.1 and ≈ 1.4 Elo at
60+0.6. At Backrank's strength a speed gain should be worth more.

- [ ] **F1. Profile.** Find where time really goes, starting with these suspects.
- [ ] **F2. Stop generating illegal moves.** 4% to 53% of moves tried at interior nodes are illegal,
  each costing a full make and unmake. The worst cases are probably check evasions, so a dedicated
  evasion generator is the first step, and a pin-aware legal generator the second. Carp reports about
  435 million perft nodes per second on a 7950X with fully legal generation, which isn't directly
  comparable with our 68 million on this Mac. Validate with perft, of course.
- [ ] **F3. Less zeroing per node.** `MoveList` is 512 bytes and `Ordered` adds 1,024 bytes of
  scores, both initialised at every interior node and every quiescence node that doesn't stand pat.
  Options include smaller lists in quiescence (a const-generic capacity), or a combined
  move-and-score array. Measure first, as this is a guess until `F1` shows it.
- [ ] **F4. Selection sort.** `Ordered` scores all moves upfront. Staged generation (`O8`) removes
  most of the cost.
- [ ] **F5. Repeated work.** `is_draw` calls `has_insufficient_material` at every node, including
  quiescence, with ten bitboard counts. A total piece count kept in `Position` would answer almost
  every case with one comparison.
- [ ] **F6. Profile-guided optimisation.** Carp says "up to 60 elo" and Reckless has `make pgo`,
  though effects vary by CPU. The rustc steps: build with `-Cprofile-generate`, run `bench`, merge
  with `llvm-profdata` (from `llvm-tools-preview`), build with `-Cprofile-use`, with identical flags
  otherwise. It fits `release.yml`, which already builds natively on each OS.
- [ ] **F7. TT prefetch.** Prefetch the entry for the child position before making the move. On
  x86-64, `_mm_prefetch` is stable and safe. On ARM, `_prefetch` is nightly-only, so the only stable
  route is inline assembly (`prfm`, needing `unsafe`). Measure on the Mac before bothering.
- [ ] **F8. Bake the magics.** `plan-v1.md` §2 notes magic numbers can become constants. Saves a
  little startup time, which helps in fast test games and on slow machines.

## Running alongside: UCI and features

Most of these are minor version bumps. None should come before the search work unless someone needs
them.

| ID | Item | Notes |
|---|---|---|
| U1 | `searchmoves` | Parsed and ignored today. Easy, with root move filtering. Analysis GUIs use it |
| U2 | `go mate` | Parsed and ignored. Stop once a mate within N moves is proven |
| U3 | MultiPV | For analysis only. It's costly: Stockfish loses 97 Elo at MultiPV 2. Off by default |
| U4 | Pondering, `ponderhit` | Hyatt puts correct predictions at about 50%. CCRL plays with pondering off, so low value for ratings, but some GUI users will expect it |
| U5 | `UCI_LimitStrength`, `UCI_Elo`, `Skill Level` | Ways to play weakly for humans. MadChess reduces knowledge and speed and picks weaker MultiPV moves. Arasan uses ply limits, slowdown and eval noise. Could calibrate against our own ladder |
| U6 | `Clear Hash`, `hashfull`, `currmove` | Small polish that GUIs show |
| U7 | `UCI_ShowWDL` | Needs a fitted model of score to win rate. A nice extra with NNUE |
| U8 | Debug commands | `eval` (a breakdown), `bench` with position lists, `see <move>` |
| U9 | Makefile with `EXE=` and `EVALFILE=` | Only if we join OpenBench (`V5`) |
| U10 | Syzygy support | See below |

**Syzygy tablebases are low priority.** With NNUE, Stockfish 15 gets only 2.7 Elo from 6-man tables,
though classical versions got about 13 to 16. The 6-man set is 149 GiB, and the 7-man set is 16.7 TiB.
The ways to do it:

- **Fathom:** C, MIT, thread-safe. Reckless uses it. It brings a C build and `unsafe` FFI, so it
  breaks the std-only rule.
- **`shakmaty-syzygy`:** Rust, GPL-3.0-or-later, closely based on Stockfish's code. It's archived and
  merged into `shakmaty`, and adds several crates.
- **Our own probing code:** keeps std-only, but it's a substantial, fiddly job.

Only worth doing if analysis users ask or a tournament needs it. Testing doesn't need it (`M7`).

**Not planned:** an opening book inside the engine. CCRL requires an own book to be disabled anyway,
and GUIs and match tools supply openings. Chess960 stays a non-goal (`plan-v1.md` §1).

## Running alongside: outside validation

`plan-release.md` §4 already lists these as more valuable than most package managers.

- [ ] **V1. Rating lists.** CCRL takes no submissions: "testers choose what and how to test". So
  publish a good release, then announce it on its forum or TalkChess. Conditions for Blitz: 2+1
  on normalised hardware, pondering off, no own book, Hash 128 or 256 MiB. So `Hash` matters here
  (`TT3`). Do this once it's stable after Stage 3 or later, as a first rating outside our own tests.
  CEGT is another list.
- [ ] **V2. Chess Programming Wiki page.**
- [ ] **V3. Lichess bot.** `lichess-bot` bridges any UCI engine. Upgrading an account to BOT is
  irreversible, so use a new account. Real opponents and game variety make it good for finding bugs.
- [ ] **V4. Tournaments.** TCEC asks developers to apply with a questionnaire, and open-source
  engines' code must be public at submission. Far off, until strength is much higher.
- [ ] **V5. OpenBench.** A self-hosted test server suits having several machines. Requirements:
  `Hash` and `Threads`, `bench` from the command line, determinism across machines, a makefile taking
  `EXE=`, and `EVALFILE=` for networks. Not worth it with one Mac, but `H1` and `U9` keep it possible.

## Correctness backlog

Nothing here has been seen to cause a problem. Each is a small known gap.

- [ ] **B1. En passant in the key.** `can_capture_en_passant` ignores pins, so a pinned capturer still
  changes the key, a small difference from FIDE's definition of a repeated position (`plan-v1.md`
  Phase 2). Stockfish 18 fixed a rare bug involving repetition, en passant and pins. Very low impact.
- [ ] **B2. Dead positions.** `has_insufficient_material` covers bare kings and a single minor piece,
  but not bishops all on one colour. Low impact.
- [ ] **B3. Quiescence in check.** It stands pat while in check and can't see mate there. See
  [Tried and rejected](#tried-and-rejected).
- [ ] **B4. Roadmap links.** The README's roadmap link and `CONTRIBUTING.md` still point to
  `docs/plan-v1.md`.

## Tried and rejected

| Idea | Result | Why it might deserve another try |
|---|---|---|
| In quiescence, don't stand pat when in check; search all evasions and score mate if none (`9092c41`) | H0 accepted, Elo -25 [-44, -6], 1,005 games at 2+0.02 | It costs depth, and depth is scarce now. It was tested at a very fast control. Try again after Stage 3 at 8+0.08, limited to a few plies, once ordering and TT make it cheaper |

Results kept for reference, from `plan-v1.md` §9: partial iterations gained +54 [+30, +78], and
tactical-only generation in quiescence gained +17 [+4, +30], both at 2+0.02.

## Ideas not yet scheduled

Add freely. These have no evidence or slot yet.

**Search**

- Quiet checks in the first ply of quiescence.
- Recapture and passed-pawn-push extensions. Older ideas, possibly covered by other extensions now.
- Limiting total check extensions per line, if the check-heavy tests or `seldepth` show waste.
- Deep futility pruning and limited razoring, as in CPW.
- A contempt setting: positive against weaker opponents. Stockfish moved away from this.

**Evaluation**

- **A small KPK bitbase,** generated at startup with std only. Exact knowledge about the most common
  simple ending, at almost no cost, without Syzygy. Interesting if `M9` confirms an endgame weakness,
  though NNUE and search depth may cover most of it.
- Specific endgame rules: wrong-colour bishop and rook pawn, KRP vs KR, and so on.
- Normalising displayed scores so that +1.00 means a fixed win chance, as Stockfish does.
- WDL-output networks.

**Testing and tools**

- Blunder-mining corpus: collapse positions from `M9` as a regression set of positions to check by eye.
- An `analyse` run for every release, published in its notes.
- A nightly or weekly long-time-control check of `main` against the last release, locally.
- STS or WAC scores purely as a diagnostic, never as a gate.
- Tracking a chart of the ladder history on the README.

**Distribution and fun**

- A WebAssembly build for playing in a browser. Reckless and Stockfish 19 both have one.
- Android and RISC-V builds (`plan-release.md` skipped them for now).
- Package managers, as `plan-release.md` §4 already researched.
- A personality or style setting for human opponents.

## Decisions to make

| # | Question | Recommendation |
|---|---|---|
| 1 | Should the TT land before v1.0.0, or after? | **After.** v1 is defined as a correct, robust classical baseline, and Phase 6 is short. Stage 3 can be the first post-1.0 minor release |
| 2 | Train networks on own data only, or also on others' (Stockfish uses Lc0 data; some engines use Stockfish data)? | **Own only.** Slower to start but keeps Backrank its own. CCRL doesn't judge originality, and no TCEC rule on this turned up in the pages read, so this is a matter of identity rather than eligibility. Viridithas and Carp both do this |
| 3 | How much handcrafted evaluation before NNUE? | **Tapered tables, tuning and passed pawns for certain;** mobility and king safety only if NNUE is still far away |
| 4 | When to start NNUE? | **After Stage 7 and the data generator (`D1`).** Stage 8's tuning and terms and all of Stage 9 are optional before it. An earlier start works, but with poorer data |
| 5 | Is a C dependency ever acceptable, for Fathom? | **No, not now.** Revisit if tablebases are requested. Std-only is a clear and useful property |
| 6 | Does Chess960 stay a non-goal? | **Yes.** It costs us DFRC data-generation openings and TCEC's FRD events, and neither matters now |
| 7 | Should each passed feature be its own release? | **Yes for now,** since releases are automatic. Batch if the release notes get noisy |
| 8 | When should the README's roadmap link point here? | **Now,** once this file is merged (`B4`) |

## Reference data

### Strength of comparable engines

From each project's README unless noted. These are CCRL ratings, and they drift as lists update: Carp
3.0.1's README says 3538, while the current list below says 3525. Many releases also changed more than
the features listed here.

| Engine | Version | Features at that point | Blitz | 40/15 |
|---|---|---|---|---|
| MinimalChess | 0.2 | Material, PV-first iterative deepening | 909 | |
| MinimalChess | 0.3 | + MVV-LVA, quiescence, PSTs | 1439 | |
| Rustic | Alpha 1 | Like Backrank v0.2.0 (`plan-v1.md` §3) | 1675 | |
| Rustic | Alpha 2 | + TT | 1815 | |
| MinimalChess | 0.4.1 | + tapered PSTs, killers, staged generation | 1816 | |
| Rustic | Alpha 3 | + killers, PVS | 1865 | |
| Reckless | v0.1.0 | | 2005 | |
| Leorik | 1.0 | PSTs, no unsafe pruning or reductions | 2112 | |
| MinimalChess | 0.5 | + mobility table, null move, simple TT | 2217 | |
| Reckless | v0.2.0 | | 2349 | |
| MinimalChess | 0.6 | + two-bucket TT, LMR, futility, history | 2443 | |
| Leorik | 2.0 | Same PSTs, + null move, futility, history, LMR | 2537 | 2529 |
| Leorik | 2.1 | + pawn structure | 2566 | 2598 |
| Reckless | v0.3.0 | | 2616 | |
| Leorik | 2.2 | + mobility | 2690 | 2681 |
| Leorik | 2.3 | All terms tuned from scratch on own games | 2677 | 2736 |
| Leorik | 2.4 | + SEE in quiescence and search | 2831 | 2799 |
| Leorik | 2.5 | Linear PSQTs, AVX2, Lazy SMP | 2921 | 2922 |
| Reckless | v0.4.0 | | 2934 | |
| Carp | 2.0.0 | First NNUE, replacing the handcrafted eval | 3159 | 3087 |
| Reckless | v0.5.0 | | 3240 | |
| Leorik | 3.0 | NNUE, 768 to 256 | 3276 | 3227 |
| Reckless | v0.6.0 | | 3386 | |
| Leorik | 3.1 | 640 neurons, SCReLU, correction history | 3430 | 3370 |
| Leorik | 3.2 | Horizontal mirroring, 5 input and 8 output buckets | 3496 | 3400 |
| Reckless | v0.7.0 | | 3500 | |
| Carp | 3.0.1 | `(768 -> 1024)x2 -> 1` | 3538 | 3502 |
| Reckless | v0.8.0 | | 3765 | 3609 |
| Reckless | v0.9.0 | | 3767 | 3612 |

**Current lists,** read directly from CCRL. Blitz, computed 2026-10-31: Stockfish 19 8CPU 3803, Reckless
0.9.0 3766, Viridithas 20.0.0 3748, Carp 3.0.1 3525, Leorik 3.2 3490. 40/15, computed 2026-11-06:
Stockfish 19 4CPU 3650, Reckless 0.9.0 4CPU 3644, Viridithas 19.0.1 4CPU 3632, Carp 3.0.0 4CPU 3502.

The search features that show up across engines with published lists, a small sample (Carp, Leorik,
MinimalChess, plus Reckless's source):

- **In nearly all:** iterative deepening, PVS, TT with aging, killers and history, null move, LMR,
  futility pruning, quiescence search.
- **In most stronger ones:** aspiration windows, staged generation, SEE, mate distance pruning, Lazy
  SMP, Syzygy, SPRT testing.
- **In the strongest:** singular extensions, multi-cut, IIR, late move pruning, history pruning,
  reverse futility pruning, continuation and capture history, correction history, NNUE with buckets,
  PGO.

### Stockfish 15's feature estimates

Read from the comments in its `search.cpp`. These are estimates for an engine far stronger than Backrank,
with everything else switched on, so they won't be what a feature is worth in an engine lacking its
neighbours. They are useful for relative priority.

| Feature | ~Elo | | Feature | ~Elo |
|---|---|---|---|---|
| **LMR** | 98 | | **Shallow pruning** in total | 98 |
| of which history in LMR | 30 | | of which LMP (move count) | 7 |
| of which cut nodes, capture hash move | 3, 3 | | of which SEE pruning | 9 |
| of which PV, TT-PV, opponent's move count | 2, 3, 1 | | of which futility (parent) | 9 |
| **Extensions** in total | 66 | | of which continuation history | 2 |
| of which singular | 58 | | of which negative SEE | 3 |
| of which check | 1 | | of which capture futility | 0 |
| Futility (child, a form of RFP) | 25 | | **Quiescence:** TT score as eval | 7 |
| Null move | 22 | | Quiescence: futility and move count | 5 |
| ProbCut | 4 | | Quiescence: negative SEE | 5 |
| TT score as eval | 4 | | Quiescence: continuation history | 2 |
| Eval difference for quiet ordering | 3 | | IIR | 3 |

### Other Stockfish measurements

| Topic | Data |
|---|---|
| Extra ply, depth N vs N-1 | 6v5 153, 7v6 131, 8v7 191, 9v8 186, 10v9 162, 11v10 126, 12v11 89, 14v13 76, 16v15 54, 18v17 52 |
| Threads, 1 to 8 | +178.6 ±14 at 60+0.6; +327 ±15 at 10+0.1; about 82% to 83% as good as 8 times the time |
| Hash, relative to 64 MiB at 60+0.6 or 240+2.4 (page doesn't say which) | 16 MiB +0.7; 8 MiB -10.7; 4 MiB -21.5; 2 MiB -29.5; 1 MiB -47.8. Advice: "keeping the average hashfull below 30% is best" |
| MultiPV cost at 60+0.6 | 2 PVs -97; 3 PVs -157; 4 PVs -199; 5 PVs -235 |
| Speed | 1% ≈ 2.10 Elo at short control, 1.43 at long |
| Turning off incremental NNUE | Nodes per second about halved, -67.6 Elo |
| Syzygy 6-man, 10+0.1 | Stockfish 6 to 11: about 15; Stockfish 15: 2.7 |

### Rust features relevant here

Checked against official documentation on 2026-11-06. The toolchain is pinned at 1.98.0, and
`rust-version` is 1.88.

| Feature | Status |
|---|---|
| `#[target_feature]` on safe functions | Stable since 1.86. Calling one from code without the feature still needs `unsafe` |
| Intrinsics safe inside matching `#[target_feature]` | Stable since 1.87, for most of them |
| NEON intrinsics | Stable since 1.59 |
| AVX-512 intrinsics and target features | Rust 1.89 release notes. Using them means raising `rust-version` |
| `std::simd` | Nightly only |
| `_mm_prefetch` (x86-64) | Stable, and safe to call |
| `_prefetch` (ARM), `prefetch_read_data` | Nightly only |
| `AtomicU64` | Lock-free on our targets. Race-free with `Relaxed`, with no ordering between words |
| `std::thread::scope` | Stable since 1.63 |
| `include_bytes!` | Gives `&[u8; N]`, alignment 1 |
| PGO | Needs `llvm-tools-preview` for `llvm-profdata` |

## Sources

Most of this was read on 2026-11-06. Some pages were only partly available:

- **Unavailable:** `syzygy-tables.info` (blocked; sizes are from CPW), the Lichess API docs (empty, so bot
  rules beyond `lichess-bot`'s are unconfirmed), the Chessdom article on Reckless (rate limited),
  Ethereal's `Tuning.pdf` (couldn't be read as text), and CPW's `Lockless_Hashing` and `MultiPV` pages (not
  found).
- **Rustic's Elo figures** come from `plan-v1.md` §3, as its progress pages couldn't be reached this time.
- **Carp's** ratings by version use column positions in its README table, which was flattened when read, and
  its 2.0.0 row should be checked.

**Search and evaluation technique**

- [Chess Programming Wiki](https://www.chessprogramming.org/): [Transposition Table](https://www.chessprogramming.org/Transposition_Table), [Aspiration Windows](https://www.chessprogramming.org/Aspiration_Windows), [Principal Variation Search](https://www.chessprogramming.org/Principal_Variation_Search), [Null Move Pruning](https://www.chessprogramming.org/Null_Move_Pruning), [Late Move Reductions](https://www.chessprogramming.org/Late_Move_Reductions), [Reverse Futility Pruning](https://www.chessprogramming.org/Reverse_Futility_Pruning), [Futility Pruning](https://www.chessprogramming.org/Futility_Pruning), [Razoring](https://www.chessprogramming.org/Razoring), [Internal Iterative Reductions](https://www.chessprogramming.org/Internal_Iterative_Reductions), [Singular Extensions](https://www.chessprogramming.org/Singular_Extensions), [Static Exchange Evaluation](https://www.chessprogramming.org/Static_Exchange_Evaluation), [History Heuristic](https://www.chessprogramming.org/History_Heuristic), [Killer Move](https://www.chessprogramming.org/Killer_Move), [Countermove Heuristic](https://www.chessprogramming.org/Countermove_Heuristic), [Correction History](https://www.chessprogramming.org/Static_Evaluation_Correction_History), [Mate Distance Pruning](https://www.chessprogramming.org/Mate_Distance_Pruning), [Time Management](https://www.chessprogramming.org/Time_Management), [Lazy SMP](https://www.chessprogramming.org/Lazy_SMP), [Texel's Tuning Method](https://www.chessprogramming.org/Texel%27s_Tuning_Method), [PeSTO](https://www.chessprogramming.org/PeSTO%27s_Evaluation_Function), [Tapered Eval](https://www.chessprogramming.org/Tapered_Eval), [Pawn Structure](https://www.chessprogramming.org/Pawn_Structure), [Mobility](https://www.chessprogramming.org/Mobility), [King Safety](https://www.chessprogramming.org/King_Safety), [Passed Pawn](https://www.chessprogramming.org/Passed_Pawn), [NNUE](https://www.chessprogramming.org/NNUE), [Syzygy Bases](https://www.chessprogramming.org/Syzygy_Bases), [Pondering](https://www.chessprogramming.org/Pondering), [Opening Book](https://www.chessprogramming.org/Opening_Book), [Engine Testing](https://www.chessprogramming.org/Engine_Testing)
- [Stockfish 15 `search.cpp`](https://github.com/official-stockfish/Stockfish/blob/sf_15/src/search.cpp) and [`position.cpp`](https://github.com/official-stockfish/Stockfish/blob/sf_15/src/position.cpp); [Stockfish useful data](https://official-stockfish.github.io/docs/stockfish-wiki/Useful-data.html); [Stockfish 18 announcement](https://stockfishchess.org/blog/2026/stockfish-18/); [Stockfish releases](https://github.com/official-stockfish/Stockfish/releases)
- Talkchess threads on [Texel tuning](https://talkchess.com/viewtopic.php?p=772585), [troubles with Texel tuning](https://talkchess.com/viewtopic.php?p=733157), [bullet training problems](https://talkchess.com/viewtopic.php?p=970140), [zero-training NNUEs](https://www.talkchess.com/viewtopic.php?p=971108) and [strength limiting (MadChess)](http://www.talkchess.com/forum/viewtopic.php?t=51973)

**Engines**

- [Reckless](https://github.com/codedeliveryservice/Reckless) (README, `src/time.rs`, `src/transposition.rs`, `src/search.rs`, `src/history.rs`, `src/nnue.rs`); [Viridithas](https://github.com/cosmobobak/viridithas) (README, [`notes/networkhistory.txt`](https://github.com/cosmobobak/viridithas/blob/master/notes/networkhistory.txt), `src/datagen.rs`); [Carp](https://github.com/dede1751/carp) (README, [`tools/src/datagen.rs`](https://github.com/dede1751/carp/blob/main/tools/src/datagen.rs), `chess/src/nnue/nnue_history.txt`)
- [Leorik](https://github.com/lithander/Leorik); [MinimalChess](https://github.com/lithander/MinimalChessEngine); [Rustic](https://rustic-chess.org) ([Codeberg repository](https://codeberg.org/mvanthoor/rustic)); [Ethereal](https://github.com/AndyGrant/Ethereal)

**Training and tuning tools**

- [bullet](https://github.com/jw1912/bullet) (README, `docs/2-getting-started.md`, `docs/3-data.md`, `examples/simple.rs`, `examples/progression/`); [NNUE documentation in nnue-pytorch](https://github.com/official-stockfish/nnue-pytorch/blob/master/docs/nnue.md)
- [texel-tuner](https://github.com/GediminasMasaitis/texel-tuner); [weather-factory](https://github.com/jnlt3/weather-factory); [SPSA on CPW](https://www.chessprogramming.org/SPSA)

**Testing and rating**

- [OpenBench wiki](https://github.com/AndyGrant/OpenBench/wiki): [SPRT workloads](https://github.com/AndyGrant/OpenBench/wiki/SPRT-and-Fixed%E2%80%90Game-Workloads), [SPSA workloads](https://github.com/AndyGrant/OpenBench/wiki/SPSA-Tuning-Workloads), [data generation](https://github.com/AndyGrant/OpenBench/wiki/Data-Generation-Workloads), [requirements for public engines](https://github.com/AndyGrant/OpenBench/wiki/Requirements-For-Public-Engines)
- [SPRT on CPW](https://www.chessprogramming.org/Sequential_Probability_Ratio_Test); [fastchess manual](https://github.com/Disservin/fastchess/blob/master/man.md); [Stockfish opening books](https://github.com/official-stockfish/books)
- [CCRL](https://computerchess.org.uk/ccrl/): [Blitz list](https://computerchess.org.uk/ccrl/404/), [Blitz conditions](https://computerchess.org.uk/ccrl/404/about.html), [40/15 list](https://computerchess.org.uk/ccrl/4040/); [TCEC rules](https://wiki.chessdom.org/Rules); [lichess-bot](https://github.com/lichess-bot-devs/lichess-bot)

**Tablebases**

- [Fathom](https://github.com/jdart1/Fathom); [shakmaty-syzygy](https://github.com/niklasf/shakmaty-syzygy)

**Rust**

- [Rust 1.86](https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/), [1.87](https://blog.rust-lang.org/2025/05/15/Rust-1.87.0/) and [1.89](https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/) release notes; [`_mm_prefetch`](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_prefetch.html); [`_prefetch`](https://doc.rust-lang.org/core/arch/aarch64/fn._prefetch.html); [`vaddq_s16`](https://doc.rust-lang.org/core/arch/aarch64/fn.vaddq_s16.html); [`std::simd`](https://doc.rust-lang.org/std/simd/index.html); [`std::sync::atomic`](https://doc.rust-lang.org/std/sync/atomic/); [`std::thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html); [`include_bytes!`](https://doc.rust-lang.org/std/macro.include_bytes.html); [profile-guided optimisation](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
