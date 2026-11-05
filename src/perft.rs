//! Perft: counting the leaf nodes of the legal move tree to a fixed depth.
//!
//! Comparing these counts against known values is the standard way to test move generation.

use crate::board::Position;
use crate::movegen::{self, MoveList};
use crate::types::Move;
use std::sync::atomic::{AtomicBool, Ordering};

/// A stop flag that is never set, for the versions that always run to completion.
static NEVER: AtomicBool = AtomicBool::new(false);

/// The number of legal move sequences of length `depth` from `pos`. `pos` is left unchanged.
pub fn perft(pos: &mut Position, depth: u32) -> u64 {
    count(pos, depth, &NEVER).expect("perft without a stop flag always finishes")
}

/// Perft that gives up, returning `None`, once `stop` is set. `pos` is left unchanged
/// either way.
fn count(pos: &mut Position, depth: u32, stop: &AtomicBool) -> Option<u64> {
    if depth == 0 {
        return Some(1);
    }
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    let mut moves = MoveList::new();
    movegen::generate(pos, &mut moves);
    let mut nodes = 0;
    for &m in moves.iter() {
        if pos.make_move(m) {
            let below = if depth == 1 {
                Some(1)
            } else {
                count(pos, depth - 1, stop)
            };
            pos.unmake_move();
            nodes += below?;
        }
    }
    Some(nodes)
}

/// Perft split by first move: each legal move with the leaf count below it, in move
/// generation order. The counts add up to `perft(pos, depth)`.
///
/// # Panics
/// If `depth` is 0.
pub fn divide(pos: &mut Position, depth: u32) -> Vec<(Move, u64)> {
    divide_until_stopped(pos, depth, &NEVER).expect("divide without a stop flag always finishes")
}

/// Like [`divide`], but gives up, returning `None`, once `stop` is set.
///
/// # Panics
/// If `depth` is 0.
pub fn divide_until_stopped(
    pos: &mut Position,
    depth: u32,
    stop: &AtomicBool,
) -> Option<Vec<(Move, u64)>> {
    assert!(depth >= 1, "divide needs a depth of at least 1");
    let mut split = Vec::new();
    for &m in movegen::legal_moves(pos).iter() {
        let made = pos.make_move(m);
        assert!(made, "legal_moves returned the illegal move {m}");
        let nodes = count(pos, depth - 1, stop);
        pos.unmake_move();
        split.push((m, nodes?));
    }
    Some(split)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divide_adds_up_to_perft() {
        let mut pos = Position::startpos();
        let split = divide(&mut pos, 3);
        assert_eq!(split.len(), 20);
        assert_eq!(split.iter().map(|&(_, n)| n).sum::<u64>(), 8_902);
        assert_eq!(perft(&mut pos, 3), 8_902);
        assert_eq!(perft(&mut pos, 0), 1);
    }

    #[test]
    fn stops_when_asked_and_leaves_the_position_unchanged() {
        let mut pos = Position::startpos();
        let stop = AtomicBool::new(true);
        assert_eq!(divide_until_stopped(&mut pos, 5, &stop), None);
        assert_eq!(pos, Position::startpos());
        stop.store(false, Ordering::Relaxed);
        assert_eq!(divide_until_stopped(&mut pos, 2, &stop).unwrap().len(), 20);
    }
}
