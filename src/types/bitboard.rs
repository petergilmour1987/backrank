//! 64-bit sets of squares.

use super::{File, Rank, Square};
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

/// A set of squares: bit `n` is set when square index `n` is in the set.
///
/// Iterating a bitboard yields its squares in ascending order (a1 first).
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Bitboard = Bitboard(0);
    pub const FULL: Bitboard = Bitboard(!0);

    pub const fn from_square(sq: Square) -> Bitboard {
        Bitboard(1 << sq.index())
    }

    /// Every square on `file`.
    pub const fn file(file: File) -> Bitboard {
        Bitboard(0x0101_0101_0101_0101 << file as u8)
    }

    /// Every square on `rank`.
    pub const fn rank(rank: Rank) -> Bitboard {
        Bitboard(0xff << (8 * rank as u8))
    }

    pub const fn contains(self, sq: Square) -> bool {
        self.0 & (1 << sq.index()) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Number of squares in the set.
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// The lowest-numbered square in the set.
    pub const fn lsb(self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            Some(Square::from_index_unchecked(self.0.trailing_zeros() as u8))
        }
    }

    /// Removes and returns the lowest-numbered square in the set.
    pub fn pop_lsb(&mut self) -> Option<Square> {
        let sq = self.lsb()?;
        self.0 &= self.0 - 1;
        Some(sq)
    }
}

// Iteration goes through a separate type rather than `impl Iterator for Bitboard`, so
// iterator methods such as `eq` or `min` can't be called on a bitboard by accident.
impl IntoIterator for Bitboard {
    type Item = Square;
    type IntoIter = Squares;

    fn into_iter(self) -> Squares {
        Squares(self)
    }
}

/// Iterator over the squares of a [`Bitboard`], in ascending order.
#[derive(Clone, Debug)]
pub struct Squares(Bitboard);

impl Iterator for Squares {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        self.0.pop_lsb()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.0.count() as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for Squares {}

impl From<Square> for Bitboard {
    fn from(sq: Square) -> Bitboard {
        Bitboard::from_square(sq)
    }
}

macro_rules! bit_op {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $op:tt) => {
        impl $trait for Bitboard {
            type Output = Bitboard;
            fn $method(self, rhs: Bitboard) -> Bitboard {
                Bitboard(self.0 $op rhs.0)
            }
        }

        impl $assign_trait for Bitboard {
            fn $assign_method(&mut self, rhs: Bitboard) {
                self.0 = self.0 $op rhs.0;
            }
        }
    };
}

bit_op!(BitAnd, bitand, BitAndAssign, bitand_assign, &);
bit_op!(BitOr, bitor, BitOrAssign, bitor_assign, |);
bit_op!(BitXor, bitxor, BitXorAssign, bitxor_assign, ^);

impl Not for Bitboard {
    type Output = Bitboard;
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

/// Prints the set as a board, rank 8 at the top: `X` for members, `.` otherwise.
impl fmt::Display for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in Rank::ALL.into_iter().rev() {
            write!(f, "{} ", rank.to_char())?;
            for file in File::ALL {
                let mark = if self.contains(Square::new(file, rank)) {
                    'X'
                } else {
                    '.'
                };
                write!(f, " {mark}")?;
            }
            writeln!(f)?;
        }
        write!(f, "   a b c d e f g h")
    }
}

impl fmt::Debug for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitboard({:#018x})\n{self}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_squares() {
        for sq in Square::all() {
            let bb = Bitboard::from_square(sq);
            assert!(bb.contains(sq));
            assert_eq!(bb.count(), 1);
            assert_eq!(bb.lsb(), Some(sq));
            assert_eq!(Bitboard::from(sq), bb);
        }
        assert_eq!(Bitboard::EMPTY.lsb(), None);
        assert!(Bitboard::EMPTY.is_empty());
        assert_eq!(Bitboard::FULL.count(), 64);
    }

    #[test]
    fn file_and_rank_masks() {
        for file in File::ALL {
            let mask = Bitboard::file(file);
            assert_eq!(mask.count(), 8);
            assert!(mask.into_iter().all(|sq| sq.file() == file));
        }
        for rank in Rank::ALL {
            let mask = Bitboard::rank(rank);
            assert_eq!(mask.count(), 8);
            assert!(mask.into_iter().all(|sq| sq.rank() == rank));
        }
    }

    #[test]
    fn iterates_in_ascending_order_and_pops() {
        let mut bb = Bitboard::from_square(Square::H8)
            | Bitboard::from_square(Square::A1)
            | Bitboard::from_square(Square::E4);
        let mut squares = bb.into_iter();
        assert_eq!(squares.len(), 3);
        assert_eq!(squares.next(), Some(Square::A1));
        assert_eq!(squares.len(), 2);
        assert_eq!(squares.collect::<Vec<_>>(), [Square::E4, Square::H8]);
        // Iterating works on a copy, so the bitboard itself is unchanged.
        assert_eq!(bb.count(), 3);
        let all: Vec<Square> = Bitboard::FULL.into_iter().collect();
        assert_eq!(all, Square::all().collect::<Vec<_>>());
        assert_eq!(bb.pop_lsb(), Some(Square::A1));
        assert_eq!(bb.pop_lsb(), Some(Square::E4));
        assert_eq!(bb.pop_lsb(), Some(Square::H8));
        assert_eq!(bb.pop_lsb(), None);
    }

    #[test]
    fn set_operations() {
        let a = Bitboard::file(File::A);
        let r1 = Bitboard::rank(Rank::R1);
        assert_eq!((a & r1).into_iter().collect::<Vec<_>>(), [Square::A1]);
        assert_eq!((a | r1).count(), 15);
        assert_eq!((a ^ r1).count(), 14);
        assert_eq!((!a).count(), 56);
        let mut x = a;
        x &= r1;
        assert_eq!(x, Bitboard::from_square(Square::A1));
        x |= Bitboard::from_square(Square::B2);
        x ^= Bitboard::from_square(Square::A1);
        assert_eq!(x, Bitboard::from_square(Square::B2));
    }

    #[test]
    fn displays_as_a_board() {
        let shown = Bitboard::from_square(Square::A8).to_string();
        let first_line = shown.lines().next().unwrap();
        assert_eq!(first_line, "8  X . . . . . . .");
        assert!(shown.ends_with("   a b c d e f g h"));
    }
}
