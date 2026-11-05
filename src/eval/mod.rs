//! Static evaluation: how good a position looks without searching.
//!
//! Scores are in centipawns (a pawn is 100) from the side to move's point of view, as
//! negamax needs. For now only material counts.

use crate::board::Position;
use crate::types::{Color, Piece, PieceType};

/// Material values in centipawns, indexed by [`PieceType::index`]. Kings are never
/// captured, so they count for nothing.
pub const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

/// The position's value for the side to move.
pub fn evaluate(pos: &Position) -> i32 {
    let mut white_minus_black = 0;
    for kind in PieceType::ALL {
        let value = PIECE_VALUES[kind.index()];
        let white = pos.pieces(Piece::new(Color::White, kind)).count() as i32;
        let black = pos.pieces(Piece::new(Color::Black, kind)).count() as i32;
        white_minus_black += value * (white - black);
    }
    match pos.side_to_move() {
        Color::White => white_minus_black,
        Color::Black => -white_minus_black,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(fen: &str) -> i32 {
        evaluate(&Position::from_fen(fen).unwrap())
    }

    #[test]
    fn start_position_is_level() {
        assert_eq!(evaluate(&Position::startpos()), 0);
    }

    #[test]
    fn scores_are_from_the_side_to_moves_view() {
        // White is a queen up.
        let white_to_move = eval("4k3/8/8/8/8/8/8/3QK3 w - - 0 1");
        let black_to_move = eval("4k3/8/8/8/8/8/8/3QK3 b - - 0 1");
        assert_eq!(white_to_move, 900);
        assert_eq!(black_to_move, -900);
    }

    #[test]
    fn counts_every_piece_type() {
        // White: R + B + N + 2P = 500 + 330 + 320 + 200. Black: Q = 900.
        assert_eq!(eval("q3k3/8/8/8/8/8/PP6/RBN1K3 w - - 0 1"), 1350 - 900);
    }
}
