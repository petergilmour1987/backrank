//! Zobrist hashing keys.
//!
//! A position's key is the XOR of one random number per (piece, square) on the board,
//! one for the castling rights, one for the en passant file (only when an en passant
//! capture is possible), and one if black is to move. Keys are generated at compile time
//! from a fixed seed, so they are the same in every build and every run.

use crate::rng::SplitMix64;

pub(crate) struct ZobristKeys {
    /// Indexed by `[piece index][square index]`.
    pub(crate) pieces: [[u64; 64]; 12],
    /// Indexed by the castling rights bit pattern. Entry 0 (no rights) is 0, so a position
    /// without castling rights contributes nothing.
    pub(crate) castling: [u64; 16],
    /// Indexed by the file of the en passant square.
    pub(crate) en_passant: [u64; 8],
    pub(crate) black_to_move: u64,
}

/// "backrank" in ASCII.
const SEED: u64 = 0x6261_636b_7261_6e6b;

pub(crate) static KEYS: ZobristKeys = ZobristKeys::generate(SEED);

impl ZobristKeys {
    const fn generate(seed: u64) -> ZobristKeys {
        let mut rng = SplitMix64::new(seed);

        let mut pieces = [[0; 64]; 12];
        let mut piece = 0;
        while piece < 12 {
            let mut square = 0;
            while square < 64 {
                pieces[piece][square] = rng.next_u64();
                square += 1;
            }
            piece += 1;
        }

        // One key per single right; each combination is the XOR of its rights' keys.
        let single = [
            rng.next_u64(),
            rng.next_u64(),
            rng.next_u64(),
            rng.next_u64(),
        ];
        let mut castling = [0; 16];
        let mut rights = 0;
        while rights < 16 {
            let mut bit = 0;
            while bit < 4 {
                if rights & (1 << bit) != 0 {
                    castling[rights] ^= single[bit];
                }
                bit += 1;
            }
            rights += 1;
        }

        let mut en_passant = [0; 8];
        let mut file = 0;
        while file < 8 {
            en_passant[file] = rng.next_u64();
            file += 1;
        }

        ZobristKeys {
            pieces,
            castling,
            en_passant,
            black_to_move: rng.next_u64(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn independent_keys_are_distinct_and_nonzero() {
        let mut keys: Vec<u64> = KEYS.pieces.iter().flatten().copied().collect();
        keys.extend([1, 2, 4, 8].map(|right| KEYS.castling[right]));
        keys.extend(KEYS.en_passant);
        keys.push(KEYS.black_to_move);
        assert_eq!(keys.len(), 12 * 64 + 4 + 8 + 1);
        assert!(keys.iter().all(|&k| k != 0));
        let unique: HashSet<u64> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len());
    }

    #[test]
    fn castling_combinations_are_xors_of_single_rights() {
        assert_eq!(KEYS.castling[0], 0);
        assert_eq!(
            KEYS.castling[15],
            KEYS.castling[1] ^ KEYS.castling[2] ^ KEYS.castling[4] ^ KEYS.castling[8]
        );
        assert_eq!(KEYS.castling[5], KEYS.castling[1] ^ KEYS.castling[4]);
    }
}
