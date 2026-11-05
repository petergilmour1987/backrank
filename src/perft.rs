//! Perft: counting the leaf nodes of the legal move tree to a fixed depth.
//!
//! Comparing these counts against known values is the standard way to test move generation.

use crate::board::Position;
use crate::movegen::{self, MoveList};
use crate::types::Move;

/// The number of legal move sequences of length `depth` from `pos`. `pos` is left unchanged.
pub fn perft(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let mut moves = MoveList::new();
    movegen::generate(pos, &mut moves);
    let mut nodes = 0;
    for &m in moves.iter() {
        if pos.make_move(m) {
            nodes += if depth == 1 { 1 } else { perft(pos, depth - 1) };
            pos.unmake_move();
        }
    }
    nodes
}

/// Perft split by first move: each legal move with the leaf count below it, in move
/// generation order. The counts add up to `perft(pos, depth)`.
///
/// # Panics
/// If `depth` is 0.
pub fn divide(pos: &mut Position, depth: u32) -> Vec<(Move, u64)> {
    assert!(depth >= 1, "divide needs a depth of at least 1");
    movegen::legal_moves(pos)
        .iter()
        .map(|&m| {
            let made = pos.make_move(m);
            assert!(made, "legal_moves returned the illegal move {m}");
            let nodes = perft(pos, depth - 1);
            pos.unmake_move();
            (m, nodes)
        })
        .collect()
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
}
