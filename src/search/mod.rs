//! Choosing a move: a negamax alpha-beta search to a fixed depth.

use crate::board::Position;
use crate::eval;
use crate::movegen::{self, MoveList};
use crate::types::Move;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The deepest the search goes, in plies from the root.
pub const MAX_PLY: usize = 128;

/// Depth searched when the GUI gives no depth. Time limits are not used yet (Phase 5
/// step 2 adds iterative deepening and time management).
const DEFAULT_DEPTH: u32 = 4;

/// The score of being checkmated at the root. Mate `n` plies from the root scores
/// `MATE - n` for the winner, so shorter mates score higher.
const MATE: i32 = 32_000;
/// Larger than any real score.
const INFINITY: i32 = MATE + 1;
/// Scores this far from zero or further are mates.
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;

/// How often, in nodes, to check the stop flag.
const CHECK_INTERVAL: u64 = 2048;

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

struct Searcher<'a> {
    pos: &'a mut Position,
    limits: &'a Limits,
    stop: &'a AtomicBool,
    nodes: u64,
    seldepth: usize,
    /// Set once the search has been told to stop; scores found after that are unreliable.
    aborted: bool,
    pv: PvTable,
}

impl<'a> Searcher<'a> {
    fn new(pos: &'a mut Position, limits: &'a Limits, stop: &'a AtomicBool) -> Searcher<'a> {
        Searcher {
            pos,
            limits,
            stop,
            nodes: 0,
            seldepth: 0,
            aborted: false,
            pv: PvTable::new(),
        }
    }

    fn run(&mut self, report: &mut dyn FnMut(&Info)) -> Option<Move> {
        let legal = movegen::legal_moves(self.pos);
        let Some(&first) = legal.first() else {
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
        };

        let depth = self.limits.depth.unwrap_or(DEFAULT_DEPTH).max(1);
        let (best, score) = self.search_root(depth);
        let (best, depth, score, pv) = match best {
            Some(best) => (best, depth, score, self.pv.root_line()),
            // Stopped before any root move was searched: fall back to the first legal
            // move, scored by the static evaluation.
            None => (first, 0, eval::evaluate(self.pos), vec![first]),
        };
        report(&Info {
            depth,
            seldepth: self.seldepth as u32,
            score: Score::from_search(score),
            nodes: self.nodes,
            time: self.limits.start.elapsed(),
            pv,
        });
        Some(best)
    }

    /// Searches every root move and returns the best one found with its score. If the
    /// search is stopped part way, the best of the moves searched so far.
    fn search_root(&mut self, depth: u32) -> (Option<Move>, i32) {
        self.pv.clear(0);
        let mut moves = MoveList::new();
        movegen::generate(self.pos, &mut moves);
        let mut alpha = -INFINITY;
        let mut best = None;
        for &m in moves.iter() {
            if !self.pos.make_move(m) {
                continue;
            }
            let score = -self.negamax(depth - 1, 1, -INFINITY, -alpha);
            self.pos.unmake_move();
            if self.aborted {
                break;
            }
            if score > alpha {
                alpha = score;
                best = Some(m);
                self.pv.update(0, m);
            }
        }
        (best, alpha)
    }

    /// The fail-soft negamax value of the current position, `ply` moves from the root,
    /// searched `depth` plies deeper, within the window (`alpha`, `beta`).
    fn negamax(&mut self, depth: u32, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        self.pv.clear(ply);
        self.nodes += 1;
        self.seldepth = self.seldepth.max(ply);
        if self.should_stop() {
            return 0;
        }
        if depth == 0 || ply >= MAX_PLY - 1 {
            return eval::evaluate(self.pos);
        }

        let mut moves = MoveList::new();
        movegen::generate(self.pos, &mut moves);
        let mut best = -INFINITY;
        let mut legal = 0;
        for &m in moves.iter() {
            if !self.pos.make_move(m) {
                continue;
            }
            legal += 1;
            let score = -self.negamax(depth - 1, ply + 1, -beta, -alpha);
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
            return if self.pos.in_check() {
                -MATE + ply as i32
            } else {
                0
            };
        }
        best
    }

    /// Whether to abandon the search: the GUI said stop, or the node limit is reached.
    fn should_stop(&mut self) -> bool {
        if self.aborted {
            return true;
        }
        if self.limits.nodes.is_some_and(|limit| self.nodes >= limit)
            || (self.nodes.is_multiple_of(CHECK_INTERVAL) && self.stop.load(Ordering::Relaxed))
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
        // Back-rank mate: Ra8#.
        let (best, info) = best_move("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 2);
        assert_eq!(best.unwrap().to_string(), "a1a8");
        assert_eq!(info.score, Score::Mate(1));
    }

    #[test]
    fn wins_material_and_avoids_losing_it() {
        // The queen on d5 hangs to the knight on c3.
        let (best, info) = best_move("4k3/8/8/3q4/8/2N5/8/4K3 w - - 0 1", 2);
        assert_eq!(best.unwrap().to_string(), "c3d5");
        assert!(
            matches!(info.score, Score::Centipawns(cp) if cp > 200),
            "{info:?}"
        );
        // With black to move, the queen must not go where the knight can take it.
        let (best, _) = best_move("4k3/8/8/3q4/8/2N5/8/4K3 b - - 0 1", 2);
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
        let mut info = None;
        let best = search(&mut pos, &limits, &AtomicBool::new(true), &mut |i| {
            info = Some(i.clone())
        });
        assert!(movegen::legal_moves(&mut pos).contains(&best.unwrap()));
        let info = info.unwrap();
        assert!(matches!(info.score, Score::Centipawns(_)), "{info:?}");
        assert_eq!(info.pv.first().copied(), best);
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
}
