//! Move generation.
//!
//! [`generate`] produces pseudo-legal moves: moves that follow how the pieces move but may
//! leave the mover's own king in check. [`Position::make_move`] rejects those, so legal
//! moves are the pseudo-legal moves that `make_move` accepts.

use crate::attacks;
use crate::board::Position;
use crate::types::{Bitboard, CastlingRights, Color, Move, Piece, PieceType, Rank, Square};
use std::fmt;
use std::ops::{Deref, DerefMut};

/// More than the most moves possible in any position (218).
pub const MAX_MOVES: usize = 256;
const CAPACITY: usize = MAX_MOVES;

/// A fixed-capacity list of moves, so generating moves never allocates.
#[derive(Clone)]
pub struct MoveList {
    moves: [Move; CAPACITY],
    len: usize,
}

impl MoveList {
    pub fn new() -> MoveList {
        MoveList {
            moves: [Move::NULL; CAPACITY],
            len: 0,
        }
    }

    pub fn push(&mut self, m: Move) {
        self.moves[self.len] = m;
        self.len += 1;
    }
}

impl Default for MoveList {
    fn default() -> MoveList {
        MoveList::new()
    }
}

impl Deref for MoveList {
    type Target = [Move];

    fn deref(&self) -> &[Move] {
        &self.moves[..self.len]
    }
}

impl DerefMut for MoveList {
    fn deref_mut(&mut self) -> &mut [Move] {
        &mut self.moves[..self.len]
    }
}

impl fmt::Debug for MoveList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Appends every pseudo-legal move in `pos` to `list`. Castling is fully checked here: the
/// king may not castle out of, through or into check.
pub fn generate(pos: &Position, list: &mut MoveList) {
    let us = pos.side_to_move();
    let own = pos.occupancy(us);
    let enemy = pos.occupancy(!us);
    let occupied = own | enemy;

    pawn_moves(pos, us, enemy, occupied, list);
    for kind in [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        for from in pos.pieces(Piece::new(us, kind)) {
            for to in piece_reach(kind, from, occupied) & !own {
                list.push(if enemy.contains(to) {
                    Move::capture(from, to)
                } else {
                    Move::quiet(from, to)
                });
            }
        }
    }
    castling_moves(pos, us, occupied, list);
}

/// The squares a piece other than a pawn attacks from `from`.
fn piece_reach(kind: PieceType, from: Square, occupied: Bitboard) -> Bitboard {
    match kind {
        PieceType::Knight => attacks::knight_attacks(from),
        PieceType::Bishop => attacks::bishop_attacks(from, occupied),
        PieceType::Rook => attacks::rook_attacks(from, occupied),
        PieceType::Queen => attacks::queen_attacks(from, occupied),
        PieceType::King => attacks::king_attacks(from),
        PieceType::Pawn => unreachable!("pawns are generated separately"),
    }
}

/// Appends the pseudo-legal moves quiescence search looks at: captures and queen promotions.
/// Underpromotions are left out, since a queen is almost always better.
pub fn generate_tactical(pos: &Position, list: &mut MoveList) {
    let us = pos.side_to_move();
    let enemy = pos.occupancy(!us);
    let occupied = pos.occupied();

    let (step, last_rank) = match us {
        Color::White => (8, Rank::R8),
        Color::Black => (-8, Rank::R1),
    };
    for from in pos.pieces(Piece::new(us, PieceType::Pawn)) {
        let attacked = attacks::pawn_attacks(us, from);
        for to in attacked & enemy {
            list.push(if to.rank() == last_rank {
                Move::promotion(from, to, PieceType::Queen, true)
            } else {
                Move::capture(from, to)
            });
        }
        if let Some(ep) = pos.en_passant()
            && attacked.contains(ep)
        {
            list.push(Move::en_passant(from, ep));
        }
        let one = from.offset(step);
        if one.rank() == last_rank && !occupied.contains(one) {
            list.push(Move::promotion(from, one, PieceType::Queen, false));
        }
    }
    for kind in [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        for from in pos.pieces(Piece::new(us, kind)) {
            for to in piece_reach(kind, from, occupied) & enemy {
                list.push(Move::capture(from, to));
            }
        }
    }
}

fn pawn_moves(pos: &Position, us: Color, enemy: Bitboard, occupied: Bitboard, list: &mut MoveList) {
    let (step, start_rank, last_rank) = match us {
        Color::White => (8, Rank::R2, Rank::R8),
        Color::Black => (-8, Rank::R7, Rank::R1),
    };
    for from in pos.pieces(Piece::new(us, PieceType::Pawn)) {
        // A pawn is never on its last rank, so one step forward is always on the board.
        let one = from.offset(step);
        if !occupied.contains(one) {
            push_pawn_move(from, one, false, last_rank, list);
            if from.rank() == start_rank {
                let two = one.offset(step);
                if !occupied.contains(two) {
                    list.push(Move::double_push(from, two));
                }
            }
        }
        let attacked = attacks::pawn_attacks(us, from);
        for to in attacked & enemy {
            push_pawn_move(from, to, true, last_rank, list);
        }
        if let Some(ep) = pos.en_passant()
            && attacked.contains(ep)
        {
            list.push(Move::en_passant(from, ep));
        }
    }
}

/// A pawn move to `to`, or all four promotions if `to` is on the last rank.
fn push_pawn_move(from: Square, to: Square, capture: bool, last_rank: Rank, list: &mut MoveList) {
    if to.rank() == last_rank {
        for kind in [
            PieceType::Queen,
            PieceType::Rook,
            PieceType::Bishop,
            PieceType::Knight,
        ] {
            list.push(Move::promotion(from, to, kind, capture));
        }
    } else if capture {
        list.push(Move::capture(from, to));
    } else {
        list.push(Move::quiet(from, to));
    }
}

struct Castle {
    right: CastlingRights,
    king_from: Square,
    king_to: Square,
    /// Squares between king and rook, which must be empty.
    between: Bitboard,
    /// Squares the king passes through or lands on, which must not be attacked.
    passes: [Square; 2],
    kingside: bool,
}

const fn squares(list: &[Square]) -> Bitboard {
    let mut bits = 0;
    let mut i = 0;
    while i < list.len() {
        bits |= 1 << list[i].index();
        i += 1;
    }
    Bitboard(bits)
}

const CASTLES: [Castle; 4] = [
    Castle {
        right: CastlingRights::WHITE_KINGSIDE,
        king_from: Square::E1,
        king_to: Square::G1,
        between: squares(&[Square::F1, Square::G1]),
        passes: [Square::F1, Square::G1],
        kingside: true,
    },
    Castle {
        right: CastlingRights::WHITE_QUEENSIDE,
        king_from: Square::E1,
        king_to: Square::C1,
        between: squares(&[Square::D1, Square::C1, Square::B1]),
        passes: [Square::D1, Square::C1],
        kingside: false,
    },
    Castle {
        right: CastlingRights::BLACK_KINGSIDE,
        king_from: Square::E8,
        king_to: Square::G8,
        between: squares(&[Square::F8, Square::G8]),
        passes: [Square::F8, Square::G8],
        kingside: true,
    },
    Castle {
        right: CastlingRights::BLACK_QUEENSIDE,
        king_from: Square::E8,
        king_to: Square::C8,
        between: squares(&[Square::D8, Square::C8, Square::B8]),
        passes: [Square::D8, Square::C8],
        kingside: false,
    },
];

fn castling_moves(pos: &Position, us: Color, occupied: Bitboard, list: &mut MoveList) {
    // No castling out of check.
    if pos.castling().is_empty() || pos.in_check() {
        return;
    }
    let them = !us;
    for castle in &CASTLES {
        // A right is only held while its king and rook are on their starting squares, so
        // the right also says whose castle this is.
        if pos.castling().contains(castle.right)
            && (castle.between & occupied).is_empty()
            && !castle
                .passes
                .iter()
                .any(|&sq| pos.is_square_attacked(sq, them))
        {
            list.push(if castle.kingside {
                Move::castle_kingside(castle.king_from, castle.king_to)
            } else {
                Move::castle_queenside(castle.king_from, castle.king_to)
            });
        }
    }
}

/// Every legal move in `pos`. `pos` is left unchanged.
pub fn legal_moves(pos: &mut Position) -> MoveList {
    let mut pseudo = MoveList::new();
    generate(pos, &mut pseudo);
    let mut legal = MoveList::new();
    for &m in pseudo.iter() {
        if pos.make_move(m) {
            pos.unmake_move();
            legal.push(m);
        }
    }
    legal
}

/// The legal move written as `text` in UCI notation (`e2e4`, `e7e8q`, `e1g1`), if any.
pub fn find_legal_move(pos: &mut Position, text: &str) -> Option<Move> {
    legal_moves(pos)
        .iter()
        .copied()
        .find(|m| m.to_string() == text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legal(fen: &str) -> Vec<String> {
        let mut pos = Position::from_fen(fen).unwrap();
        let mut moves: Vec<String> = legal_moves(&mut pos).iter().map(Move::to_string).collect();
        moves.sort();
        moves
    }

    #[test]
    fn start_position_has_twenty_moves() {
        let moves = legal(Position::STARTPOS_FEN);
        assert_eq!(moves.len(), 20);
        assert!(moves.contains(&"e2e4".to_string()));
        assert!(moves.contains(&"g1f3".to_string()));
    }

    #[test]
    fn castling_needs_empty_and_safe_squares() {
        // Black rook on f2 attacks f1: white may castle queenside but not kingside.
        let moves = legal("r3k2r/8/8/8/8/8/5r2/R3K2R w KQkq - 0 1");
        assert!(moves.contains(&"e1c1".to_string()));
        assert!(!moves.contains(&"e1g1".to_string()));
        // b1 may be attacked (the king doesn't pass it) but must be empty.
        let attacked_b1 = legal("r3k2r/8/8/8/8/8/1r6/R3K2R w KQkq - 0 1");
        assert!(attacked_b1.contains(&"e1c1".to_string()));
        let blocked_b1 = legal("r3k2r/8/8/8/8/8/8/RN2K2R w KQkq - 0 1");
        assert!(!blocked_b1.contains(&"e1c1".to_string()));
        // No castling out of check.
        let in_check = legal("r3k2r/8/8/8/8/8/4r3/R3K2R w KQkq - 0 1");
        assert!(!in_check.iter().any(|m| m == "e1g1" || m == "e1c1"));
    }

    #[test]
    fn en_passant_that_exposes_the_king_is_illegal() {
        // Taking d5xe6 would leave the white king on a5 in check from the h5 rook.
        let moves = legal("4k3/8/8/K2pP2r/8/8/8/8 w - d6 0 1");
        assert!(!moves.contains(&"e5d6".to_string()));
        let moves = legal("4k3/8/8/3pP3/8/8/8/K7 w - d6 0 1");
        assert!(moves.contains(&"e5d6".to_string()));
    }

    #[test]
    fn promotions_come_in_fours() {
        let moves = legal("3n3k/2P5/8/8/8/8/8/K7 w - - 0 1");
        for m in ["c7c8q", "c7c8r", "c7c8b", "c7c8n", "c7d8q", "c7d8n"] {
            assert!(moves.contains(&m.to_string()), "missing {m}");
        }
        assert_eq!(moves.iter().filter(|m| m.starts_with("c7")).count(), 8);
    }

    /// Checks `generate_tactical` against the full generator, filtered, at every position
    /// of a walk through the legal move tree.
    fn check_tactical(pos: &mut Position, depth: u32) {
        let mut all = MoveList::new();
        generate(pos, &mut all);
        let mut expected: Vec<Move> = all
            .iter()
            .copied()
            .filter(|m| match m.promotion_piece() {
                Some(piece) => piece == PieceType::Queen,
                None => m.is_capture(),
            })
            .collect();
        let mut tactical = MoveList::new();
        generate_tactical(pos, &mut tactical);
        let mut actual: Vec<Move> = tactical.to_vec();
        let key = |m: &Move| m.to_string();
        expected.sort_by_key(key);
        actual.sort_by_key(key);
        assert_eq!(actual, expected, "in {}", pos.to_fen());
        if depth > 0 {
            for &m in all.iter() {
                if pos.make_move(m) {
                    check_tactical(pos, depth - 1);
                    pos.unmake_move();
                }
            }
        }
    }

    #[test]
    fn tactical_moves_match_the_filtered_full_generator() {
        for fen in [
            Position::STARTPOS_FEN,
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        ] {
            check_tactical(&mut Position::from_fen(fen).unwrap(), 2);
        }
    }

    #[test]
    fn finds_legal_moves_by_name() {
        let mut pos = Position::startpos();
        let m = find_legal_move(&mut pos, "e2e4").unwrap();
        assert!(m.is_double_push());
        assert_eq!(find_legal_move(&mut pos, "e2e5"), None);
        assert_eq!(find_legal_move(&mut pos, "e1g1"), None);
        let mut promo = Position::from_fen("3n3k/2P5/8/8/8/8/8/K7 w - - 0 1").unwrap();
        assert!(find_legal_move(&mut promo, "c7c8q").is_some());
        assert_eq!(find_legal_move(&mut promo, "c7c8"), None);
    }
}
