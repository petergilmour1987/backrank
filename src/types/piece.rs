//! Colours, piece types and coloured pieces.

use std::fmt;
use std::ops::Not;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub const ALL: [Color; 2] = [Color::White, Color::Black];

    /// 0 for white, 1 for black.
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn opposite(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

/// `white` or `black`.
impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Color::White => "white",
            Color::Black => "black",
        })
    }
}

impl Not for Color {
    type Output = Color;
    fn not(self) -> Color {
        self.opposite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PieceType {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceType {
    pub const ALL: [PieceType; 6] = [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ];

    /// 0 for pawn through 5 for king.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Parses a lowercase piece letter: `p`, `n`, `b`, `r`, `q` or `k`.
    pub fn from_char(c: char) -> Option<PieceType> {
        Some(match c {
            'p' => PieceType::Pawn,
            'n' => PieceType::Knight,
            'b' => PieceType::Bishop,
            'r' => PieceType::Rook,
            'q' => PieceType::Queen,
            'k' => PieceType::King,
            _ => return None,
        })
    }

    /// The lowercase piece letter, as used for promotions in UCI moves.
    pub const fn to_char(self) -> char {
        match self {
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
        }
    }
}

/// A piece of a given colour. Indices run 0..12: white pawn to white king are 0..6,
/// black pawn to black king are 6..12.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece(u8);

impl Piece {
    pub const fn new(color: Color, kind: PieceType) -> Piece {
        Piece(color as u8 * 6 + kind as u8)
    }

    pub const fn color(self) -> Color {
        if self.0 < 6 {
            Color::White
        } else {
            Color::Black
        }
    }

    pub const fn kind(self) -> PieceType {
        PieceType::ALL[(self.0 % 6) as usize]
    }

    /// 0..12, see the type documentation.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// All 12 pieces in index order.
    pub fn all() -> impl Iterator<Item = Piece> {
        (0..12).map(Piece)
    }

    /// Parses a FEN piece letter: uppercase for white, lowercase for black.
    pub fn from_fen_char(c: char) -> Option<Piece> {
        let kind = PieceType::from_char(c.to_ascii_lowercase())?;
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        Some(Piece::new(color, kind))
    }

    /// The FEN piece letter: uppercase for white, lowercase for black.
    pub const fn to_fen_char(self) -> char {
        let c = self.kind().to_char();
        match self.color() {
            Color::White => c.to_ascii_uppercase(),
            Color::Black => c,
        }
    }
}

impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_fen_char())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_opposites() {
        assert_eq!(!Color::White, Color::Black);
        assert_eq!(Color::Black.opposite(), Color::White);
    }

    #[test]
    fn pieces_combine_and_split() {
        let mut seen = 0;
        for color in Color::ALL {
            for kind in PieceType::ALL {
                let piece = Piece::new(color, kind);
                assert_eq!(piece.color(), color);
                assert_eq!(piece.kind(), kind);
                assert_eq!(piece.index(), color.index() * 6 + kind.index());
                seen += 1;
            }
        }
        assert_eq!(seen, Piece::all().count());
    }

    #[test]
    fn fen_chars_round_trip() {
        let chars: String = Piece::all().map(Piece::to_fen_char).collect();
        assert_eq!(chars, "PNBRQKpnbrqk");
        for piece in Piece::all() {
            assert_eq!(Piece::from_fen_char(piece.to_fen_char()), Some(piece));
        }
        assert_eq!(Piece::from_fen_char('x'), None);
        assert_eq!(Piece::from_fen_char('1'), None);
    }
}
