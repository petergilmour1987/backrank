//! Choosing a move.
//!
//! For now this plays a uniformly random legal move. That is enough to exercise the UCI
//! protocol and the match harness; the real search replaces it in Phase 5 behind the same
//! interface.

use crate::board::Position;
use crate::movegen;
use crate::rng::SplitMix64;
use crate::types::Move;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

/// Progress reported while searching, for the GUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub depth: u32,
    pub score: Score,
    pub nodes: u64,
    pub time: Duration,
    /// The expected line of play, best move first.
    pub pv: Vec<Move>,
}

/// Picks the move to play in `pos`, or `None` if there are no legal moves. `pos` is left
/// unchanged. Progress goes to `report`. Returns early if `stop` is set, and in infinite
/// mode waits for it.
///
/// For now the move is random and the reported score is always 0 (or mate 0 when
/// checkmated): nothing is evaluated yet.
pub fn search(
    pos: &mut Position,
    limits: &Limits,
    stop: &AtomicBool,
    report: &mut dyn FnMut(&Info),
) -> Option<Move> {
    let moves = movegen::legal_moves(pos);
    let choice = if moves.is_empty() {
        None
    } else {
        // RandomState is seeded randomly per process, which gives a seed without a
        // dependency or a clock.
        let mut rng = SplitMix64::new(RandomState::new().hash_one(pos.key()));
        Some(moves[(rng.next_u64() % moves.len() as u64) as usize])
    };
    report(&Info {
        depth: u32::from(choice.is_some()),
        score: if choice.is_none() && pos.in_check() {
            Score::Mate(0)
        } else {
            Score::Centipawns(0)
        },
        nodes: moves.len() as u64,
        time: limits.start.elapsed(),
        pv: choice.into_iter().collect(),
    });
    // In infinite mode the GUI expects no `bestmove` until it sends `stop`.
    if limits.infinite {
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn plays_a_legal_move_and_varies() {
        let stop = AtomicBool::new(false);
        let mut pos = Position::startpos();
        let mut seen = HashSet::new();
        for _ in 0..50 {
            let m = search(&mut pos, &Limits::default(), &stop, &mut |_| {}).unwrap();
            assert!(movegen::legal_moves(&mut pos).contains(&m));
            seen.insert(m);
        }
        assert!(
            seen.len() > 5,
            "only {} different moves in 50 tries",
            seen.len()
        );
        assert_eq!(pos, Position::startpos());
    }

    #[test]
    fn no_move_when_mated_or_stalemated() {
        let stop = AtomicBool::new(false);
        let mut reports = Vec::new();
        let mut mated = Position::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let result = search(&mut mated, &Limits::default(), &stop, &mut |i| {
            reports.push(i.clone())
        });
        assert_eq!(result, None);
        let mut stalemate = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let result = search(&mut stalemate, &Limits::default(), &stop, &mut |i| {
            reports.push(i.clone())
        });
        assert_eq!(result, None);
        let scores: Vec<Score> = reports.iter().map(|i| i.score).collect();
        assert_eq!(scores, [Score::Mate(0), Score::Centipawns(0)]);
        assert!(reports.iter().all(|i| i.pv.is_empty() && i.depth == 0));
    }
}
