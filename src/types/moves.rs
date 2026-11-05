//! Moves packed into 16 bits.

use super::{PieceType, Square};
use std::fmt;

// The 4-bit flag in bits 12-15, following chessprogramming.org's "Encoding Moves".
// Bit 2 (value 4) marks captures and bit 3 (value 8) marks promotions; for promotions the
// low two bits give the piece (0 knight, 1 bishop, 2 rook, 3 queen).
const QUIET: u16 = 0;
const DOUBLE_PUSH: u16 = 1;
const KING_CASTLE: u16 = 2;
const QUEEN_CASTLE: u16 = 3;
const CAPTURE: u16 = 4;
const EN_PASSANT: u16 = 5;
const PROMOTION: u16 = 8;

const PROMOTION_PIECES: [PieceType; 4] = [
    PieceType::Knight,
    PieceType::Bishop,
    PieceType::Rook,
    PieceType::Queen,
];

/// A move: from square in bits 0-5, to square in bits 6-11, and a flag in bits 12-15
/// saying what kind of move it is. Castling is stored as the king's move (`e1g1`).
///
/// A `Move` does not record which piece moves or what it captures; that comes from the
/// position it is played in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Move(u16);

impl Move {
    /// "No move", written `0000` in UCI. No real move has equal from and to squares.
    pub const NULL: Move = Move(0);

    const fn with_flag(from: Square, to: Square, flag: u16) -> Move {
        Move(from.index() as u16 | (to.index() as u16) << 6 | flag << 12)
    }

    pub const fn quiet(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, QUIET)
    }

    /// A pawn's two-square advance from its starting rank.
    pub const fn double_push(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, DOUBLE_PUSH)
    }

    pub const fn capture(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, CAPTURE)
    }

    /// `to` is the en passant square, not the square of the captured pawn.
    pub const fn en_passant(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, EN_PASSANT)
    }

    /// The king's move, e.g. e1 to g1.
    pub const fn castle_kingside(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, KING_CASTLE)
    }

    /// The king's move, e.g. e1 to c1.
    pub const fn castle_queenside(from: Square, to: Square) -> Move {
        Move::with_flag(from, to, QUEEN_CASTLE)
    }

    /// A promotion, optionally capturing.
    ///
    /// # Panics
    /// If `piece` is a pawn or a king.
    pub const fn promotion(from: Square, to: Square, piece: PieceType, capture: bool) -> Move {
        let piece_bits = match piece {
            PieceType::Knight => 0,
            PieceType::Bishop => 1,
            PieceType::Rook => 2,
            PieceType::Queen => 3,
            PieceType::Pawn | PieceType::King => panic!("can only promote to N, B, R or Q"),
        };
        let capture_bit = if capture { CAPTURE } else { 0 };
        Move::with_flag(from, to, PROMOTION | capture_bit | piece_bits)
    }

    pub const fn from(self) -> Square {
        Square::from_index_unchecked((self.0 & 0x3f) as u8)
    }

    pub const fn to(self) -> Square {
        Square::from_index_unchecked((self.0 >> 6 & 0x3f) as u8)
    }

    const fn flag(self) -> u16 {
        self.0 >> 12
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// True for every capture, including en passant and capturing promotions.
    pub const fn is_capture(self) -> bool {
        self.flag() & CAPTURE != 0
    }

    pub const fn is_en_passant(self) -> bool {
        self.flag() == EN_PASSANT
    }

    pub const fn is_double_push(self) -> bool {
        self.flag() == DOUBLE_PUSH
    }

    pub const fn is_castle(self) -> bool {
        matches!(self.flag(), KING_CASTLE | QUEEN_CASTLE)
    }

    pub const fn is_kingside_castle(self) -> bool {
        self.flag() == KING_CASTLE
    }

    /// The piece a pawn promotes to, if this is a promotion.
    pub const fn promotion_piece(self) -> Option<PieceType> {
        if self.flag() & PROMOTION != 0 {
            Some(PROMOTION_PIECES[(self.flag() & 3) as usize])
        } else {
            None
        }
    }
}

/// UCI long algebraic notation: `e2e4`, `e7e8q`, `e1g1`, or `0000` for the null move.
impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return write!(f, "0000");
        }
        write!(f, "{}{}", self.from(), self.to())?;
        if let Some(piece) = self.promotion_piece() {
            write!(f, "{}", piece.to_char())?;
        }
        Ok(())
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_squares() {
        for from in Square::all() {
            for to in Square::all() {
                let m = Move::quiet(from, to);
                assert_eq!((m.from(), m.to()), (from, to));
            }
        }
    }

    #[test]
    fn flags_are_distinguished() {
        let (a, b) = (Square::E2, Square::E4);
        let quiet = Move::quiet(a, b);
        assert!(!quiet.is_capture() && !quiet.is_castle() && !quiet.is_double_push());
        assert_eq!(quiet.promotion_piece(), None);

        assert!(Move::double_push(a, b).is_double_push());
        assert!(!Move::double_push(a, b).is_capture());

        assert!(Move::capture(a, b).is_capture());
        assert!(!Move::capture(a, b).is_en_passant());

        let ep = Move::en_passant(Square::E5, Square::D6);
        assert!(ep.is_capture() && ep.is_en_passant());

        let oo = Move::castle_kingside(Square::E1, Square::G1);
        let ooo = Move::castle_queenside(Square::E1, Square::C1);
        assert!(oo.is_castle() && oo.is_kingside_castle() && !oo.is_capture());
        assert!(ooo.is_castle() && !ooo.is_kingside_castle());
    }

    #[test]
    fn promotions() {
        for piece in PROMOTION_PIECES {
            for capture in [false, true] {
                let m = Move::promotion(Square::B7, Square::A8, piece, capture);
                assert_eq!(m.promotion_piece(), Some(piece));
                assert_eq!(m.is_capture(), capture);
                assert!(!m.is_castle() && !m.is_en_passant() && !m.is_double_push());
            }
        }
    }

    #[test]
    #[should_panic(expected = "can only promote")]
    fn cannot_promote_to_king() {
        Move::promotion(Square::E7, Square::E8, PieceType::King, false);
    }

    #[test]
    fn uci_notation() {
        assert_eq!(Move::quiet(Square::G1, Square::F3).to_string(), "g1f3");
        assert_eq!(
            Move::promotion(Square::E7, Square::E8, PieceType::Queen, false).to_string(),
            "e7e8q"
        );
        assert_eq!(
            Move::promotion(Square::B2, Square::A1, PieceType::Knight, true).to_string(),
            "b2a1n"
        );
        assert_eq!(
            Move::castle_kingside(Square::E8, Square::G8).to_string(),
            "e8g8"
        );
        assert_eq!(Move::NULL.to_string(), "0000");
        assert!(Move::NULL.is_null());
        assert!(!Move::quiet(Square::A1, Square::B1).is_null());
    }
}
