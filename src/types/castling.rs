//! Castling rights.

use std::fmt;

/// A set of the four castling rights, stored as 4 bits.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CastlingRights(u8);

impl CastlingRights {
    pub const NONE: CastlingRights = CastlingRights(0);
    pub const WHITE_KINGSIDE: CastlingRights = CastlingRights(1);
    pub const WHITE_QUEENSIDE: CastlingRights = CastlingRights(2);
    pub const BLACK_KINGSIDE: CastlingRights = CastlingRights(4);
    pub const BLACK_QUEENSIDE: CastlingRights = CastlingRights(8);
    pub const ALL: CastlingRights = CastlingRights(15);

    /// Each single right with its FEN letter, in FEN order.
    const LETTERS: [(CastlingRights, char); 4] = [
        (Self::WHITE_KINGSIDE, 'K'),
        (Self::WHITE_QUEENSIDE, 'Q'),
        (Self::BLACK_KINGSIDE, 'k'),
        (Self::BLACK_QUEENSIDE, 'q'),
    ];

    /// True if every right in `rights` is in `self`.
    pub const fn contains(self, rights: CastlingRights) -> bool {
        self.0 & rights.0 == rights.0
    }

    /// The rights in either set.
    pub const fn union(self, other: CastlingRights) -> CastlingRights {
        CastlingRights(self.0 | other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, rights: CastlingRights) {
        self.0 |= rights.0;
    }

    pub fn remove(&mut self, rights: CastlingRights) {
        self.0 &= !rights.0;
    }

    /// 0..16: the bit pattern, for table lookups.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Parses the FEN castling field: `-`, or one or more of `KQkq` with no repeats.
    pub fn from_fen(field: &str) -> Option<CastlingRights> {
        if field == "-" {
            return Some(Self::NONE);
        }
        if field.is_empty() {
            return None;
        }
        let mut rights = Self::NONE;
        for c in field.chars() {
            let &(right, _) = Self::LETTERS.iter().find(|&&(_, letter)| letter == c)?;
            if rights.contains(right) {
                return None;
            }
            rights.insert(right);
        }
        Some(rights)
    }
}

/// The FEN castling field, in the canonical `KQkq` order, or `-`.
impl fmt::Display for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "-");
        }
        for (right, letter) in Self::LETTERS {
            if self.contains(right) {
                write!(f, "{letter}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CastlingRights({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_fen_field() {
        for field in ["-", "K", "Qk", "KQkq", "kq", "Kq"] {
            assert_eq!(CastlingRights::from_fen(field).unwrap().to_string(), field);
        }
        // Accepted in any order, printed canonically.
        assert_eq!(
            CastlingRights::from_fen("qkQK").unwrap(),
            CastlingRights::ALL
        );
    }

    #[test]
    fn rejects_bad_fields() {
        for bad in ["", "KK", "x", "K-", "--", "KQkqK", "kQ1"] {
            assert_eq!(CastlingRights::from_fen(bad), None, "{bad:?} parsed");
        }
    }

    #[test]
    fn insert_remove_contains() {
        let mut rights = CastlingRights::NONE;
        rights.insert(CastlingRights::WHITE_KINGSIDE);
        rights.insert(CastlingRights::BLACK_QUEENSIDE);
        assert!(rights.contains(CastlingRights::WHITE_KINGSIDE));
        assert!(!rights.contains(CastlingRights::WHITE_QUEENSIDE));
        rights.remove(CastlingRights::WHITE_KINGSIDE);
        assert_eq!(rights, CastlingRights::BLACK_QUEENSIDE);
        assert_eq!(rights.index(), 8);
        assert!(CastlingRights::ALL.contains(rights));
    }
}
