//! Magic bitboards for bishop and rook attacks.
//!
//! For each square, the occupancy of the squares that can block the piece (its "relevant
//! mask") is multiplied by a magic number. The top bits of the product then index a table
//! holding the attacks for that occupancy. This uses the "fancy" layout: each square has
//! its own shift and an offset into one shared table (about 840 KiB).
//!
//! Magic numbers are found at startup by trial and error, with a fixed seed so every run
//! builds the same tables. The tables are built on first use.

use crate::rng::SplitMix64;
use crate::types::{Bitboard, Square};
use std::sync::OnceLock;

const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// "magics" in ASCII.
const SEED: u64 = 0x6d61_6769_6373;

#[derive(Clone, Copy, Default)]
struct Magic {
    mask: u64,
    magic: u64,
    shift: u32,
    offset: usize,
}

impl Magic {
    fn index(&self, occupied: u64) -> usize {
        self.offset + ((occupied & self.mask).wrapping_mul(self.magic) >> self.shift) as usize
    }
}

struct Tables {
    rook: [Magic; 64],
    bishop: [Magic; 64],
    attacks: Vec<Bitboard>,
}

static TABLES: OnceLock<Tables> = OnceLock::new();

fn tables() -> &'static Tables {
    TABLES.get_or_init(Tables::build)
}

pub(super) fn init() {
    tables();
}

pub(super) fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = tables();
    t.attacks[t.rook[sq.index()].index(occupied.0)]
}

pub(super) fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = tables();
    t.attacks[t.bishop[sq.index()].index(occupied.0)]
}

impl Tables {
    fn build() -> Tables {
        let mut rng = SplitMix64::new(SEED);
        let mut attacks = Vec::new();
        let mut rook = [Magic::default(); 64];
        let mut bishop = [Magic::default(); 64];
        for sq in Square::all() {
            rook[sq.index()] = find_magic(sq, &ROOK_DIRECTIONS, &mut rng, &mut attacks);
        }
        for sq in Square::all() {
            bishop[sq.index()] = find_magic(sq, &BISHOP_DIRECTIONS, &mut rng, &mut attacks);
        }
        Tables {
            rook,
            bishop,
            attacks,
        }
    }
}

/// Attacks found by walking each direction until the edge or the first occupied square
/// (which is included). Slow; used to fill the tables and to test them.
fn sliding_attacks(sq: Square, occupied: u64, directions: &[(i8, i8); 4]) -> u64 {
    let mut attacks = 0;
    for &(df, dr) in directions {
        let (mut f, mut r) = (sq.file().index() as i8 + df, sq.rank().index() as i8 + dr);
        while (f as u8) < 8 && (r as u8) < 8 {
            let bit = 1u64 << (r * 8 + f);
            attacks |= bit;
            if occupied & bit != 0 {
                break;
            }
            f += df;
            r += dr;
        }
    }
    attacks
}

/// The squares whose occupancy can change the attacks from `sq`: every square on the rays
/// except the last one, since a piece on the edge blocks nothing beyond it.
fn relevant_mask(sq: Square, directions: &[(i8, i8); 4]) -> u64 {
    let mut mask = 0;
    for &(df, dr) in directions {
        let (mut f, mut r) = (sq.file().index() as i8 + df, sq.rank().index() as i8 + dr);
        while ((f + df) as u8) < 8 && ((r + dr) as u8) < 8 {
            mask |= 1u64 << (r * 8 + f);
            f += df;
            r += dr;
        }
    }
    mask
}

/// Finds a magic number for `sq` and appends the square's attack table to `table`.
fn find_magic(
    sq: Square,
    directions: &[(i8, i8); 4],
    rng: &mut SplitMix64,
    table: &mut Vec<Bitboard>,
) -> Magic {
    let mask = relevant_mask(sq, directions);
    let bits = mask.count_ones();
    let size = 1usize << bits;

    // Every subset of the mask, enumerated with the Carry-Rippler trick, with its attacks.
    let mut occupancies = Vec::with_capacity(size);
    let mut reference = Vec::with_capacity(size);
    let mut subset = 0u64;
    loop {
        occupancies.push(subset);
        reference.push(sliding_attacks(sq, subset, directions));
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 {
            break;
        }
    }

    let offset = table.len();
    table.resize(offset + size, Bitboard::EMPTY);
    // `filled_in[i] == attempt` means entry i was written during the current attempt, so the
    // table doesn't need clearing between attempts.
    let mut filled_in = vec![0u32; size];
    let mut attempt = 0;
    loop {
        // Sparse candidates (few bits set) are far more likely to work.
        let magic = rng.next_u64() & rng.next_u64() & rng.next_u64();
        // A magic must spread the mask's bits into the top of the product; this cheap test
        // rejects most candidates that don't.
        if (mask.wrapping_mul(magic) >> 56).count_ones() < 6 {
            continue;
        }
        attempt += 1;
        let candidate = Magic {
            mask,
            magic,
            shift: 64 - bits,
            offset,
        };
        let mut collision = false;
        for (&occupied, &attacks) in occupancies.iter().zip(&reference) {
            let i = candidate.index(occupied) - offset;
            if filled_in[i] != attempt {
                filled_in[i] = attempt;
                table[offset + i] = Bitboard(attacks);
            } else if table[offset + i].0 != attacks {
                // Two occupancies with different attacks share an entry.
                collision = true;
                break;
            }
        }
        if !collision {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_ray_walking_for_random_occupancies() {
        let mut rng = SplitMix64::new(42);
        for sq in Square::all() {
            for _ in 0..2000 {
                // Mix sparse and dense boards.
                let occupied = match rng.next_u64() % 3 {
                    0 => rng.next_u64() & rng.next_u64(),
                    1 => rng.next_u64(),
                    _ => rng.next_u64() | rng.next_u64(),
                };
                assert_eq!(
                    rook_attacks(sq, Bitboard(occupied)).0,
                    sliding_attacks(sq, occupied, &ROOK_DIRECTIONS),
                    "rook on {sq}, occupied {occupied:#x}"
                );
                assert_eq!(
                    bishop_attacks(sq, Bitboard(occupied)).0,
                    sliding_attacks(sq, occupied, &BISHOP_DIRECTIONS),
                    "bishop on {sq}, occupied {occupied:#x}"
                );
            }
        }
    }

    #[test]
    fn relevant_masks_have_the_known_sizes() {
        // Rooks: 12 bits in the corners, 10 in the middle; bishops: 6 in the corners, 9 in
        // the centre.
        let rook = |sq: Square| relevant_mask(sq, &ROOK_DIRECTIONS).count_ones();
        let bishop = |sq: Square| relevant_mask(sq, &BISHOP_DIRECTIONS).count_ones();
        assert_eq!(rook(Square::A1), 12);
        assert_eq!(rook(Square::D4), 10);
        assert_eq!(bishop(Square::A1), 6);
        assert_eq!(bishop(Square::D4), 9);
        // The fancy layout's total size: 102,400 rook entries plus 5,248 bishop entries.
        assert_eq!(tables().attacks.len(), 102_400 + 5_248);
    }
}
