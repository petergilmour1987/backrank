//! Attack tables: the squares a piece on a given square attacks.
//!
//! Knight, king and pawn attacks are fixed per square and are built at compile time.
//! Bishop and rook attacks depend on which squares are occupied and come from magic
//! bitboard tables (see the `magic` submodule).

mod magic;

use crate::types::{Bitboard, Color, Square};

const KNIGHT_STEPS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];
const KING_STEPS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const WHITE_PAWN_STEPS: [(i8, i8); 2] = [(-1, 1), (1, 1)];
const BLACK_PAWN_STEPS: [(i8, i8); 2] = [(-1, -1), (1, -1)];

/// For each square, the squares reached by one of the (file, rank) `steps`.
const fn step_table(steps: &[(i8, i8)]) -> [Bitboard; 64] {
    let mut table = [Bitboard::EMPTY; 64];
    let mut sq = 0;
    while sq < 64 {
        let (file, rank) = ((sq % 8) as i8, (sq / 8) as i8);
        let mut bits = 0u64;
        let mut i = 0;
        while i < steps.len() {
            let (f, r) = (file + steps[i].0, rank + steps[i].1);
            // Casting to u8 turns negative values into large ones, so one test covers both edges.
            if (f as u8) < 8 && (r as u8) < 8 {
                bits |= 1 << (r * 8 + f) as u32;
            }
            i += 1;
        }
        table[sq] = Bitboard(bits);
        sq += 1;
    }
    table
}

static KNIGHT: [Bitboard; 64] = step_table(&KNIGHT_STEPS);
static KING: [Bitboard; 64] = step_table(&KING_STEPS);
/// Indexed by `[color][square]`.
static PAWN: [[Bitboard; 64]; 2] = [step_table(&WHITE_PAWN_STEPS), step_table(&BLACK_PAWN_STEPS)];

/// Builds the magic bitboard tables now rather than on first use, so the cost (a few
/// milliseconds) doesn't land in the middle of a search. Calling it again does nothing.
pub fn init() {
    magic::init();
}

pub fn knight_attacks(sq: Square) -> Bitboard {
    KNIGHT[sq.index()]
}

pub fn king_attacks(sq: Square) -> Bitboard {
    KING[sq.index()]
}

/// The squares a `color` pawn on `sq` attacks (the diagonals it captures onto).
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    PAWN[color.index()][sq.index()]
}

/// The squares a bishop on `sq` attacks, given the `occupied` squares. Each ray stops at
/// (and includes) the first occupied square, whatever colour the piece on it is.
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    magic::bishop_attacks(sq, occupied)
}

/// The squares a rook on `sq` attacks; see [`bishop_attacks`].
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    magic::rook_attacks(sq, occupied)
}

/// The squares a queen on `sq` attacks; see [`bishop_attacks`].
pub fn queen_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squares(names: &[&str]) -> Bitboard {
        names
            .iter()
            .map(|n| Bitboard::from_square(n.parse().unwrap()))
            .fold(Bitboard::EMPTY, |acc, bb| acc | bb)
    }

    #[test]
    fn knight_attacks_in_corner_and_centre() {
        assert_eq!(knight_attacks(Square::A1), squares(&["b3", "c2"]));
        assert_eq!(knight_attacks(Square::D4).count(), 8);
        assert_eq!(knight_attacks(Square::H8), squares(&["g6", "f7"]));
    }

    #[test]
    fn king_attacks_in_corner_and_centre() {
        assert_eq!(king_attacks(Square::A1), squares(&["a2", "b1", "b2"]));
        assert_eq!(king_attacks(Square::E4).count(), 8);
        assert_eq!(king_attacks(Square::H5).count(), 5);
    }

    #[test]
    fn pawn_attacks_by_colour() {
        assert_eq!(
            pawn_attacks(Color::White, Square::E4),
            squares(&["d5", "f5"])
        );
        assert_eq!(
            pawn_attacks(Color::Black, Square::E4),
            squares(&["d3", "f3"])
        );
        assert_eq!(pawn_attacks(Color::White, Square::A2), squares(&["b3"]));
        assert_eq!(pawn_attacks(Color::Black, Square::H7), squares(&["g6"]));
    }

    #[test]
    fn sliders_stop_at_blockers() {
        let occupied = squares(&["d6", "f4", "b2"]);
        assert_eq!(
            rook_attacks(Square::D4, occupied),
            squares(&["d5", "d6", "d3", "d2", "d1", "e4", "f4", "c4", "b4", "a4"])
        );
        assert_eq!(
            bishop_attacks(Square::D4, occupied),
            squares(&[
                "e5", "f6", "g7", "h8", "c5", "b6", "a7", "e3", "f2", "g1", "c3", "b2"
            ])
        );
        assert_eq!(
            queen_attacks(Square::D4, occupied),
            rook_attacks(Square::D4, occupied) | bishop_attacks(Square::D4, occupied)
        );
        assert_eq!(rook_attacks(Square::A1, Bitboard::EMPTY).count(), 14);
        assert_eq!(bishop_attacks(Square::A1, Bitboard::EMPTY).count(), 7);
    }
}
