//! Choosing a move: iterative deepening negamax alpha-beta search with quiescence search,
//! move ordering and time management.
//!
//! The search goes one ply deeper each iteration and reports after each. An iteration that
//! is stopped before its first root move finishes is thrown away. Depth 1 is never
//! stopped, so there is always a move.

mod ordering;

use crate::board::Position;
use crate::eval;
use crate::movegen::{self, MoveList};
use crate::types::Move;
use ordering::Ordered;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The deepest the search goes, in plies from the root.
pub const MAX_PLY: usize = 128;

/// The deepest iteration, leaving room below it within [`MAX_PLY`].
const MAX_DEPTH: u32 = 64;

/// The score of being checkmated at the root. Mate `n` plies from the root scores
/// `MATE - n` for the winner, so shorter mates score higher.
const MATE: i32 = 32_000;
/// Larger than any real score.
const INFINITY: i32 = MATE + 1;
/// Scores this far from zero or further are mates.
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;

/// How often, in nodes, to check the stop flag and the clock.
const CHECK_INTERVAL: u64 = 2048;

/// Without `movestogo`, plan for this many more moves.
const MOVES_TO_PLAN_FOR: u32 = 20;

/// What the GUI allows for this move. Clock times are for the side to move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// When the GUI asked for the move. Time is measured from here, not from when the
    /// search thread got going.
    pub start: Instant,
    /// Time left on our clock.
    pub time_left: Option<Duration>,
    /// Our increment per move.
    pub increment: Duration,
    /// Moves until the next time control, if the GUI says.
    pub moves_to_go: Option<u32>,
    /// Exact time to spend on this move.
    pub move_time: Option<Duration>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    /// Search until `stop` is set, whatever the other limits say.
    pub infinite: bool,
    /// Time to keep in reserve for communication delays.
    pub move_overhead: Duration,
}

impl Limits {
    /// True if something other than `stop` will end the search: a clock, a fixed move
    /// time, a depth or a node count.
    pub fn is_bounded(&self) -> bool {
        self.time_left.is_some()
            || self.move_time.is_some()
            || self.depth.is_some()
            || self.nodes.is_some()
    }
}

/// No limits at all, starting now.
impl Default for Limits {
    fn default() -> Limits {
        Limits {
            start: Instant::now(),
            time_left: None,
            increment: Duration::ZERO,
            moves_to_go: None,
            move_time: None,
            depth: None,
            nodes: None,
            infinite: false,
            move_overhead: Duration::ZERO,
        }
    }
}

/// A position's value from the side to move's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Centipawns(i32),
    /// Mate in this many moves: positive when the side to move mates, negative when it is
    /// being mated, and 0 when it is already checkmated.
    Mate(i32),
}

impl Score {
    /// Converts a search score to centipawns or moves to mate.
    fn from_search(score: i32) -> Score {
        if score >= MATE_BOUND {
            Score::Mate((MATE - score + 1) / 2)
        } else if score <= -MATE_BOUND {
            Score::Mate(-(MATE + score) / 2)
        } else {
            Score::Centipawns(score)
        }
    }
}

/// Progress reported while searching, for the GUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub depth: u32,
    /// The deepest ply reached.
    pub seldepth: u32,
    pub score: Score,
    pub nodes: u64,
    pub time: Duration,
    /// The expected line of play, best move first.
    pub pv: Vec<Move>,
}

/// Picks the move to play in `pos`, or `None` if there are no legal moves. `pos` is left
/// unchanged. Progress goes to `report`. Returns early if `stop` is set, and in infinite
/// mode waits for it.
pub fn search(
    pos: &mut Position,
    limits: &Limits,
    stop: &AtomicBool,
    report: &mut dyn FnMut(&Info),
) -> Option<Move> {
    let mut searcher = Searcher::new(pos, limits, stop);
    let best = searcher.run(report);
    // In infinite mode the GUI expects no `bestmove` until it sends `stop`.
    if limits.infinite {
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    best
}

/// How long to think, as `(soft, hard)`: no new iteration starts after `soft`, and the
/// search is abandoned at `hard`. `None` when only `stop`, a depth or a node count ends
/// the search.
///
/// With a clock, the hard limit is `usable / n + increment / 2`, never more than `usable`,
/// where `usable` is the time left minus Move Overhead and `n` is `movestogo + 1` (at most
/// 20, and 20 when not given). Starting a new iteration after half of that is rarely worth
/// it, since each iteration takes several times longer than the last.
fn time_budget(limits: &Limits) -> Option<(Duration, Duration)> {
    if limits.infinite {
        return None;
    }
    if let Some(move_time) = limits.move_time {
        let time = move_time.saturating_sub(limits.move_overhead);
        return Some((time, time));
    }
    let left = limits.time_left?;
    let usable = left.saturating_sub(limits.move_overhead);
    let moves = match limits.moves_to_go {
        Some(n) => (n.max(1) + 1).min(MOVES_TO_PLAN_FOR),
        None => MOVES_TO_PLAN_FOR,
    };
    let hard = (usable / moves + limits.increment / 2).min(usable);
    Some((hard / 2, hard))
}

/// The principal variation found at each ply ("triangular" PV table): row `ply` holds the
/// best line found from that ply, in `moves[ply][ply..len[ply]]`.
struct PvTable {
    moves: Box<[[Move; MAX_PLY]; MAX_PLY]>,
    len: [usize; MAX_PLY],
}

impl PvTable {
    fn new() -> PvTable {
        PvTable {
            moves: Box::new([[Move::NULL; MAX_PLY]; MAX_PLY]),
            len: [0; MAX_PLY],
        }
    }

    /// Starts an empty line at `ply`.
    fn clear(&mut self, ply: usize) {
        self.len[ply] = ply;
    }

    /// Records `m` followed by the line found below it as the best line at `ply`.
    fn update(&mut self, ply: usize, m: Move) {
        self.moves[ply][ply] = m;
        let below = if ply + 1 < MAX_PLY {
            self.len[ply + 1]
        } else {
            ply + 1
        };
        for i in ply + 1..below {
            self.moves[ply][i] = self.moves[ply + 1][i];
        }
        self.len[ply] = below.max(ply + 1);
    }

    fn root_line(&self) -> Vec<Move> {
        self.moves[0][..self.len[0]].to_vec()
    }
}

/// What searching the root moves found.
struct RootResult {
    best: Option<Move>,
    score: i32,
    /// Whether at least the first move was searched in full.
    first_finished: bool,
}

struct Searcher<'a> {
    pos: &'a mut Position,
    limits: &'a Limits,
    stop: &'a AtomicBool,
    nodes: u64,
    seldepth: usize,
    /// No new iteration starts after this.
    soft_deadline: Option<Instant>,
    /// The search is abandoned at this.
    hard_deadline: Option<Instant>,
    /// False during depth 1, which always completes.
    can_abort: bool,
    /// Set once the search has been told to stop; scores found after that are unreliable.
    aborted: bool,
    pv: PvTable,
    /// The principal variation of the last completed iteration, tried first in the next.
    prev_pv: Vec<Move>,
}

impl<'a> Searcher<'a> {
    fn new(pos: &'a mut Position, limits: &'a Limits, stop: &'a AtomicBool) -> Searcher<'a> {
        let budget = time_budget(limits);
        Searcher {
            pos,
            limits,
            stop,
            nodes: 0,
            seldepth: 0,
            soft_deadline: budget.map(|(soft, _)| limits.start + soft),
            hard_deadline: budget.map(|(_, hard)| limits.start + hard),
            can_abort: false,
            aborted: false,
            pv: PvTable::new(),
            prev_pv: Vec::new(),
        }
    }

    fn run(&mut self, report: &mut dyn FnMut(&Info)) -> Option<Move> {
        if movegen::legal_moves(self.pos).is_empty() {
            // Checkmated or stalemated: nothing to search.
            report(&Info {
                depth: 0,
                seldepth: 0,
                score: if self.pos.in_check() {
                    Score::Mate(0)
                } else {
                    Score::Centipawns(0)
                },
                nodes: 0,
                time: self.limits.start.elapsed(),
                pv: Vec::new(),
            });
            return None;
        }

        let max_depth = self.limits.depth.unwrap_or(MAX_DEPTH).clamp(1, MAX_DEPTH);
        let mut best = None;
        for depth in 1..=max_depth {
            self.can_abort = depth > 1;
            self.seldepth = 0;
            let root = self.search_root(depth);
            // A stopped iteration still counts if the previous best move, tried first, was
            // searched in full: any move that then beat it has an exact, better score.
            if self.aborted && !root.first_finished {
                break;
            }
            best = root.best;
            report(&Info {
                depth,
                seldepth: self.seldepth as u32,
                score: Score::from_search(root.score),
                nodes: self.nodes,
                time: self.limits.start.elapsed(),
                pv: self.pv.root_line(),
            });
            if self.aborted {
                break;
            }
            self.prev_pv = self.pv.root_line();
            // Between iterations, stop for any reason the node checks would.
            if self.stop.load(Ordering::Relaxed)
                || self.soft_deadline.is_some_and(|d| Instant::now() >= d)
                || self.limits.nodes.is_some_and(|limit| self.nodes >= limit)
            {
                break;
            }
        }
        // Depth 1 always completes, and there are legal moves.
        Some(best.expect("depth 1 always finds a move"))
    }

    /// Searches every root move and returns the best one with its score. If the search is
    /// stopped part way, the result covers only the moves that finished.
    fn search_root(&mut self, depth: u32) -> RootResult {
        self.pv.clear(0);
        let mut moves = MoveList::new();
        movegen::generate(self.pos, &mut moves);
        let mut alpha = -INFINITY;
        let mut best = None;
        let mut first_finished = false;
        let pv_move = self.prev_pv.first().copied();
        for m in Ordered::new(self.pos, moves, pv_move) {
            if !self.pos.make_move(m) {
                continue;
            }
            let score = -self.negamax(depth - 1, 1, -INFINITY, -alpha, Some(m) == pv_move);
            self.pos.unmake_move();
            if self.aborted {
                break;
            }
            first_finished = true;
            if score > alpha {
                alpha = score;
                best = Some(m);
                self.pv.update(0, m);
            }
        }
        RootResult {
            best,
            score: alpha,
            first_finished,
        }
    }

    /// The fail-soft negamax value of the current position, `ply` moves from the root,
    /// searched `depth` plies deeper, within the window (`alpha`, `beta`). `on_pv` says
    /// whether every move so far followed the previous iteration's principal variation.
    fn negamax(&mut self, depth: u32, ply: usize, mut alpha: i32, beta: i32, on_pv: bool) -> i32 {
        // Check extension: search one ply deeper when in check. Quiescence only looks at
        // captures, so it can't judge a check: it wouldn't even notice checkmate.
        let in_check = self.pos.in_check();
        let depth = depth + u32::from(in_check);
        if depth == 0 {
            return self.quiescence(ply, alpha, beta);
        }
        self.pv.clear(ply);
        self.nodes += 1;
        self.seldepth = self.seldepth.max(ply);
        if self.should_stop() {
            return 0;
        }
        if self.is_draw() {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            return eval::evaluate(self.pos);
        }

        let mut moves = MoveList::new();
        movegen::generate(self.pos, &mut moves);
        // Only while on the old principal variation is its next move worth trying first.
        let pv_move = if on_pv {
            self.prev_pv.get(ply).copied()
        } else {
            None
        };
        let mut best = -INFINITY;
        let mut legal = 0;
        for m in Ordered::new(self.pos, moves, pv_move) {
            if !self.pos.make_move(m) {
                continue;
            }
            legal += 1;
            let score = -self.negamax(depth - 1, ply + 1, -beta, -alpha, Some(m) == pv_move);
            self.pos.unmake_move();
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.pv.update(ply, m);
                    if score >= beta {
                        break;
                    }
                }
            }
        }
        if legal == 0 {
            // Checkmate (the shorter the better for the winner) or stalemate.
            return if in_check { -MATE + ply as i32 } else { 0 };
        }
        best
    }

    /// The value of the current position once captures have played out: the side to move
    /// may "stand pat" on the static evaluation, or try a capture (or queen promotion) to
    /// do better. This keeps the search from stopping in the middle of an exchange.
    fn quiescence(&mut self, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        self.pv.clear(ply);
        self.nodes += 1;
        self.seldepth = self.seldepth.max(ply);
        if self.should_stop() {
            return 0;
        }
        if self.is_draw() {
            return 0;
        }
        let stand_pat = eval::evaluate(self.pos);
        if stand_pat >= beta || ply >= MAX_PLY - 1 {
            return stand_pat;
        }
        alpha = alpha.max(stand_pat);

        let mut tactical = MoveList::new();
        movegen::generate_tactical(self.pos, &mut tactical);
        let mut best = stand_pat;
        for m in Ordered::new(self.pos, tactical, None) {
            if !self.pos.make_move(m) {
                continue;
            }
            let score = -self.quiescence(ply + 1, -beta, -alpha);
            self.pos.unmake_move();
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.pv.update(ply, m);
                    if score >= beta {
                        break;
                    }
                }
            }
        }
        best
    }

    /// Whether the current position (never the root) is a draw by repetition, the
    /// fifty-move rule or insufficient material. Checkmate takes precedence over the
    /// fifty-move rule.
    fn is_draw(&mut self) -> bool {
        self.pos.is_repetition()
            || self.pos.has_insufficient_material()
            || (self.pos.halfmove_clock() >= 100
                && !(self.pos.in_check() && movegen::legal_moves(self.pos).is_empty()))
    }

    /// Whether to abandon the search: the GUI said stop, time is up, or the node limit is
    /// reached. Never during depth 1.
    fn should_stop(&mut self) -> bool {
        if self.aborted {
            return true;
        }
        if !self.can_abort {
            return false;
        }
        let out_of_nodes = self.limits.nodes.is_some_and(|limit| self.nodes >= limit);
        if out_of_nodes
            || (self.nodes.is_multiple_of(CHECK_INTERVAL)
                && (self.stop.load(Ordering::Relaxed)
                    || self.hard_deadline.is_some_and(|d| Instant::now() >= d)))
        {
            self.aborted = true;
        }
        self.aborted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best_move(fen: &str, depth: u32) -> (Option<Move>, Info) {
        let mut pos = Position::from_fen(fen).unwrap();
        let limits = Limits {
            depth: Some(depth),
            ..Limits::default()
        };
        let mut last = None;
        let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |info| {
            last = Some(info.clone())
        });
        assert_eq!(
            pos,
            Position::from_fen(fen).unwrap(),
            "search changed the position"
        );
        (best, last.expect("search reports at least once"))
    }

    #[test]
    fn mate_scores_count_moves_not_plies() {
        assert_eq!(Score::from_search(MATE - 1), Score::Mate(1));
        assert_eq!(Score::from_search(MATE - 3), Score::Mate(2));
        assert_eq!(Score::from_search(-MATE + 2), Score::Mate(-1));
        assert_eq!(Score::from_search(-MATE + 4), Score::Mate(-2));
        assert_eq!(Score::from_search(250), Score::Centipawns(250));
        assert_eq!(
            Score::from_search(-MATE_BOUND + 1),
            Score::Centipawns(-MATE_BOUND + 1)
        );
    }

    #[test]
    fn finds_mate_in_one() {
        // Back-rank mate: Ra8#. The check extension sees it even at depth 1.
        let (best, info) = best_move("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 1);
        assert_eq!(best.unwrap().to_string(), "a1a8");
        assert_eq!(info.score, Score::Mate(1));
    }

    #[test]
    fn wins_material_and_avoids_losing_it() {
        // The queen on d5 hangs to the knight on c3. (The e2 pawn keeps the result from
        // being a dead draw of king and knight against king.)
        let (best, info) = best_move("4k3/8/8/3q4/8/2N5/4P3/4K3 w - - 0 1", 2);
        assert_eq!(best.unwrap().to_string(), "c3d5");
        assert!(
            matches!(info.score, Score::Centipawns(cp) if cp > 200),
            "{info:?}"
        );
        // With black to move, the queen must not go where the knight can take it.
        let (best, _) = best_move("4k3/8/8/3q4/8/2N5/4P3/4K3 b - - 0 1", 2);
        let m = best.unwrap().to_string();
        let knight_takes = ["d5b5", "d5e4", "d5d1", "d5a4", "d5b1", "d5e2", "d5a2"];
        assert!(!knight_takes.contains(&m.as_str()), "{m}");
    }

    #[test]
    fn reports_the_principal_variation() {
        let (best, info) = best_move(Position::STARTPOS_FEN, 3);
        assert_eq!(info.depth, 3);
        assert_eq!(info.pv.first().copied(), best);
        assert_eq!(info.pv.len(), 3, "{:?}", info.pv);
        assert!(info.nodes > 0 && info.seldepth >= 3);
    }

    #[test]
    fn quiescence_sees_through_exchanges() {
        // Nxd5 wins a pawn only if nothing recaptures. Here the c6 pawn recaptures, so a
        // 1-ply search must see the exchange (N for P) and not take.
        let (best, _) = best_move("4k3/8/2p5/3p4/8/2N5/8/4K3 w - - 0 1", 1);
        assert_ne!(best.unwrap().to_string(), "c3d5");
        // Undefended, the pawn is free.
        let (best, info) = best_move("4k3/8/8/3p4/8/2N5/4P3/4K3 w - - 0 1", 1);
        assert_eq!(best.unwrap().to_string(), "c3d5");
        assert!(
            matches!(info.score, Score::Centipawns(cp) if cp >= 400),
            "{info:?}"
        );
    }

    #[test]
    fn the_fifty_move_rule_is_a_draw() {
        // A rook up, but every move reaches the hundredth half-move without a capture.
        let (_, info) = best_move("8/8/8/4k3/8/8/8/R3K3 w - - 99 80", 4);
        assert_eq!(info.score, Score::Centipawns(0));
        let (_, info) = best_move("8/8/8/4k3/8/8/8/R3K3 w - - 90 80", 4);
        assert!(
            matches!(info.score, Score::Centipawns(cp) if cp >= 400),
            "{info:?}"
        );
    }

    #[test]
    fn the_losing_side_takes_a_repetition() {
        // White is a queen up. After Qb2 Ka7 Qb1, black can go back to a8 and repeat the
        // position, which scores 0 instead of losing.
        let tokens: Vec<&str> = "fen k7/8/8/8/8/8/8/KQ6 w - - 0 1 moves b1b2 a8a7 b2b1"
            .split_whitespace()
            .collect();
        let mut pos = crate::uci::parse_position(&tokens).unwrap();
        let limits = Limits {
            depth: Some(3),
            ..Limits::default()
        };
        let mut info = None;
        let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |i| {
            info = Some(i.clone())
        });
        assert_eq!(best.unwrap().to_string(), "a7a8");
        assert_eq!(info.unwrap().score, Score::Centipawns(0));
    }

    #[test]
    fn stalemate_and_checkmate_have_no_move() {
        let (best, info) = best_move("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1", 3);
        assert_eq!((best, info.score), (None, Score::Mate(0)));
        let (best, info) = best_move("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", 3);
        assert_eq!((best, info.score), (None, Score::Centipawns(0)));
    }

    #[test]
    fn stops_promptly_and_still_returns_a_legal_move() {
        let mut pos = Position::startpos();
        let limits = Limits {
            depth: Some(30),
            ..Limits::default()
        };
        let mut infos = Vec::new();
        let best = search(&mut pos, &limits, &AtomicBool::new(true), &mut |i| {
            infos.push(i.clone())
        });
        assert!(movegen::legal_moves(&mut pos).contains(&best.unwrap()));
        // Depth 1 completes even when told to stop at once; nothing deeper does.
        assert_eq!(infos.len(), 1, "{infos:?}");
        assert_eq!(infos[0].depth, 1);
        assert_eq!(infos[0].pv.first().copied(), best);
    }

    #[test]
    fn node_limit_is_respected() {
        let mut pos = Position::startpos();
        let limits = Limits {
            depth: Some(30),
            nodes: Some(5_000),
            ..Limits::default()
        };
        let mut nodes = 0;
        search(&mut pos, &limits, &AtomicBool::new(false), &mut |info| {
            nodes = info.nodes
        });
        assert!(nodes <= 5_000, "{nodes}");
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn time_budget_from_clocks() {
        let clock = |left, inc, moves_to_go| Limits {
            time_left: Some(ms(left)),
            increment: ms(inc),
            moves_to_go,
            move_overhead: ms(20),
            ..Limits::default()
        };
        // 60 s left, no increment: (60000 - 20) / 20 = 2999 ms hard, half of it soft.
        assert_eq!(
            time_budget(&clock(60_000, 0, None)),
            Some((ms(2999) / 2, ms(2999)))
        );
        // Increment adds half of itself.
        assert_eq!(
            time_budget(&clock(8_000, 80, None)).unwrap().1,
            ms(399 + 40)
        );
        // One move to go: half of what's left, never all of it.
        assert_eq!(time_budget(&clock(1_020, 0, Some(1))).unwrap().1, ms(500));
        // `movestogo 0` is treated as 1.
        assert_eq!(time_budget(&clock(1_020, 0, Some(0))).unwrap().1, ms(500));
        // A big increment can't plan to spend more than is on the clock.
        assert_eq!(time_budget(&clock(100, 1_000, None)).unwrap().1, ms(80));
        // A flagged clock leaves nothing (depth 1 still completes).
        assert_eq!(time_budget(&clock(10, 0, None)), Some((ms(0), ms(0))));
    }

    #[test]
    fn time_budget_for_other_limits() {
        let fixed = Limits {
            move_time: Some(ms(500)),
            move_overhead: ms(20),
            ..Limits::default()
        };
        assert_eq!(time_budget(&fixed), Some((ms(480), ms(480))));
        let infinite = Limits {
            infinite: true,
            time_left: Some(ms(1_000)),
            ..Limits::default()
        };
        assert_eq!(time_budget(&infinite), None);
        let depth_only = Limits {
            depth: Some(5),
            ..Limits::default()
        };
        assert_eq!(time_budget(&depth_only), None);
    }

    #[test]
    fn deepens_one_ply_at_a_time() {
        let mut pos = Position::startpos();
        let limits = Limits {
            depth: Some(4),
            ..Limits::default()
        };
        let mut depths = Vec::new();
        search(&mut pos, &limits, &AtomicBool::new(false), &mut |i| {
            depths.push(i.depth)
        });
        assert_eq!(depths, [1, 2, 3, 4]);
    }

    #[test]
    fn keeps_to_the_move_time() {
        let mut pos = Position::from_fen(
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        )
        .unwrap();
        let limits = Limits {
            move_time: Some(ms(100)),
            ..Limits::default()
        };
        let started = Instant::now();
        let mut deepest = 0;
        search(&mut pos, &limits, &AtomicBool::new(false), &mut |i| {
            deepest = i.depth
        });
        let took = started.elapsed();
        assert!(took >= ms(90) && took < ms(400), "took {took:?}");
        assert!(deepest >= 2, "only reached depth {deepest}");
    }
}
