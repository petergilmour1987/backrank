//! Files, ranks and squares.

use std::fmt;
use std::str::FromStr;

/// A file (column), `a` through `h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum File {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
}

impl File {
    pub const ALL: [File; 8] = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];

    /// 0 for the a-file through 7 for the h-file.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The file with the given index, or `None` if `index >= 8`.
    pub const fn from_index(index: usize) -> Option<File> {
        if index < 8 {
            Some(Self::ALL[index])
        } else {
            None
        }
    }

    /// Parses a lowercase file letter.
    pub fn from_char(c: char) -> Option<File> {
        match c {
            'a'..='h' => Some(Self::ALL[(c as u8 - b'a') as usize]),
            _ => None,
        }
    }

    pub const fn to_char(self) -> char {
        (b'a' + self as u8) as char
    }
}

/// A rank (row), `1` through `8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
}

impl Rank {
    pub const ALL: [Rank; 8] = [
        Rank::R1,
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8,
    ];

    /// 0 for the first rank through 7 for the eighth.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The rank with the given index, or `None` if `index >= 8`.
    pub const fn from_index(index: usize) -> Option<Rank> {
        if index < 8 {
            Some(Self::ALL[index])
        } else {
            None
        }
    }

    /// Parses a rank digit.
    pub fn from_char(c: char) -> Option<Rank> {
        match c {
            '1'..='8' => Some(Self::ALL[(c as u8 - b'1') as usize]),
            _ => None,
        }
    }

    pub const fn to_char(self) -> char {
        (b'1' + self as u8) as char
    }
}

/// A board square, numbered 0..64 in little-endian rank-file order:
/// a1 = 0, b1 = 1, …, h1 = 7, a2 = 8, …, h8 = 63.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(u8);

impl Square {
    pub const fn new(file: File, rank: Rank) -> Square {
        Square(rank as u8 * 8 + file as u8)
    }

    /// The square with the given index, or `None` if `index >= 64`.
    pub const fn from_index(index: usize) -> Option<Square> {
        if index < 64 {
            Some(Square(index as u8))
        } else {
            None
        }
    }

    /// Callers guarantee `index < 64`.
    pub(crate) const fn from_index_unchecked(index: u8) -> Square {
        debug_assert!(index < 64);
        Square(index)
    }

    /// 0..64, see the type documentation for the numbering.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The square `delta` index steps away (+8 is one rank up). Callers guarantee the
    /// result is on the board.
    pub(crate) const fn offset(self, delta: i8) -> Square {
        Square::from_index_unchecked(self.0.wrapping_add_signed(delta))
    }

    pub const fn file(self) -> File {
        File::ALL[(self.0 & 7) as usize]
    }

    pub const fn rank(self) -> Rank {
        Rank::ALL[(self.0 >> 3) as usize]
    }

    /// All 64 squares, a1 first and h8 last.
    pub fn all() -> impl Iterator<Item = Square> {
        (0..64).map(Square)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.file().to_char(), self.rank().to_char())
    }
}

impl fmt::Debug for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Returned when a string is not a square name such as `e4`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseSquareError(pub String);

impl fmt::Display for ParseSquareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid square {:?}", self.0)
    }
}

impl std::error::Error for ParseSquareError {}

impl FromStr for Square {
    type Err = ParseSquareError;

    fn from_str(s: &str) -> Result<Square, ParseSquareError> {
        let mut chars = s.chars();
        match (
            chars.next().and_then(File::from_char),
            chars.next().and_then(Rank::from_char),
            chars.next(),
        ) {
            (Some(file), Some(rank), None) => Ok(Square::new(file, rank)),
            _ => Err(ParseSquareError(s.to_string())),
        }
    }
}

macro_rules! square_consts {
    ($($name:ident = $index:expr),* $(,)?) => {
        impl Square {
            $(pub const $name: Square = Square($index);)*
        }
    };
}

square_consts! {
    A1 = 0, B1 = 1, C1 = 2, D1 = 3, E1 = 4, F1 = 5, G1 = 6, H1 = 7,
    A2 = 8, B2 = 9, C2 = 10, D2 = 11, E2 = 12, F2 = 13, G2 = 14, H2 = 15,
    A3 = 16, B3 = 17, C3 = 18, D3 = 19, E3 = 20, F3 = 21, G3 = 22, H3 = 23,
    A4 = 24, B4 = 25, C4 = 26, D4 = 27, E4 = 28, F4 = 29, G4 = 30, H4 = 31,
    A5 = 32, B5 = 33, C5 = 34, D5 = 35, E5 = 36, F5 = 37, G5 = 38, H5 = 39,
    A6 = 40, B6 = 41, C6 = 42, D6 = 43, E6 = 44, F6 = 45, G6 = 46, H6 = 47,
    A7 = 48, B7 = 49, C7 = 50, D7 = 51, E7 = 52, F7 = 53, G7 = 54, H7 = 55,
    A8 = 56, B8 = 57, C8 = 58, D8 = 59, E8 = 60, F8 = 61, G8 = 62, H8 = 63,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_file_and_rank_agree() {
        for (i, sq) in Square::all().enumerate() {
            assert_eq!(sq.index(), i);
            assert_eq!(Square::new(sq.file(), sq.rank()), sq);
            assert_eq!(Square::from_index(i), Some(sq));
        }
        assert_eq!(Square::all().count(), 64);
        assert_eq!(Square::from_index(64), None);
    }

    #[test]
    fn named_constants_match_coordinates() {
        assert_eq!(Square::A1, Square::new(File::A, Rank::R1));
        assert_eq!(Square::H1, Square::new(File::H, Rank::R1));
        assert_eq!(Square::E4, Square::new(File::E, Rank::R4));
        assert_eq!(Square::A8, Square::new(File::A, Rank::R8));
        assert_eq!(Square::H8.index(), 63);
    }

    #[test]
    fn names_round_trip() {
        for sq in Square::all() {
            assert_eq!(sq.to_string().parse::<Square>(), Ok(sq));
        }
        assert_eq!(Square::E4.to_string(), "e4");
        assert_eq!(format!("{:?}", Square::G7), "g7");
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["", "e", "i1", "a9", "a0", "E4", "e44", "4e"] {
            assert!(bad.parse::<Square>().is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn file_and_rank_chars() {
        for file in File::ALL {
            assert_eq!(File::from_char(file.to_char()), Some(file));
            assert_eq!(File::from_index(file.index()), Some(file));
        }
        for rank in Rank::ALL {
            assert_eq!(Rank::from_char(rank.to_char()), Some(rank));
            assert_eq!(Rank::from_index(rank.index()), Some(rank));
        }
        assert_eq!(File::from_char('i'), None);
        assert_eq!(Rank::from_char('9'), None);
        assert_eq!(File::from_index(8), None);
    }
}
