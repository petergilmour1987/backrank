//! Making and unmaking moves.

use super::Position;
use super::zobrist::KEYS;
use crate::types::{CastlingRights, Color, Move, Piece, PieceType, Square};

/// What `unmake_move` needs to restore that the move itself doesn't record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Undo {
    mv: Move,
    captured: Option<Piece>,
    castling: CastlingRights,
    en_passant: Option<Square>,
    halfmove_clock: u16,
    /// The key before the move, restored as-is instead of being updated back. Also the
    /// key history that repetition detection searches.
    pub(super) key: u64,
}

/// The castling rights lost when a piece moves from or onto each square: moving the king or
/// a rook, or capturing a rook on its starting square.
static RIGHTS_LOST: [CastlingRights; 64] = {
    let mut lost = [CastlingRights::NONE; 64];
    lost[Square::A1.index()] = CastlingRights::WHITE_QUEENSIDE;
    lost[Square::H1.index()] = CastlingRights::WHITE_KINGSIDE;
    lost[Square::E1.index()] =
        CastlingRights::WHITE_KINGSIDE.union(CastlingRights::WHITE_QUEENSIDE);
    lost[Square::A8.index()] = CastlingRights::BLACK_QUEENSIDE;
    lost[Square::H8.index()] = CastlingRights::BLACK_KINGSIDE;
    lost[Square::E8.index()] =
        CastlingRights::BLACK_KINGSIDE.union(CastlingRights::BLACK_QUEENSIDE);
    lost
};

impl Position {
    /// Plays `m`, which must be pseudo-legal here (as produced by
    /// [`movegen::generate`](crate::movegen::generate)).
    ///
    /// Returns `false`, leaving the position unchanged, if the move would leave the mover's
    /// own king in check.
    ///
    /// Passing a move that is not pseudo-legal here (for example one generated in a
    /// different position) panics or corrupts the position.
    #[must_use = "an illegal move returns false and is not played"]
    pub fn make_move(&mut self, m: Move) -> bool {
        let us = self.side_to_move;
        let them = !us;
        let (from, to) = (m.from(), m.to());
        let piece = self.mailbox[from.index()]
            .unwrap_or_else(|| panic!("make_move {m}: no piece on {from}"));
        debug_assert_eq!(
            piece.color(),
            us,
            "make_move {m}: moving the opponent's piece"
        );

        let undo = Undo {
            mv: m,
            captured: None,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            key: self.key,
        };

        if let Some(ep) = self.en_passant.take() {
            self.key ^= KEYS.en_passant[ep.file().index()];
        }

        let captured = if m.is_en_passant() {
            Some(self.remove_piece(en_passant_victim(to, us)))
        } else if m.is_capture() {
            Some(self.remove_piece(to))
        } else {
            None
        };

        match m.promotion_piece() {
            Some(kind) => {
                self.remove_piece(from);
                self.put_piece(to, Piece::new(us, kind));
            }
            None => self.move_piece(from, to),
        }
        if m.is_castle() {
            let (rook_from, rook_to) = castling_rook_move(to);
            self.move_piece(rook_from, rook_to);
        }

        let old_rights = self.castling;
        self.castling
            .remove(RIGHTS_LOST[from.index()].union(RIGHTS_LOST[to.index()]));
        self.key ^= KEYS.castling[old_rights.index()] ^ KEYS.castling[self.castling.index()];

        self.halfmove_clock = if piece.kind() == PieceType::Pawn || captured.is_some() {
            0
        } else {
            self.halfmove_clock.saturating_add(1)
        };

        if m.is_double_push() {
            let ep = Square::from_index_unchecked(((from.index() + to.index()) / 2) as u8);
            if self.can_capture_en_passant(ep, them) {
                self.en_passant = Some(ep);
                self.key ^= KEYS.en_passant[ep.file().index()];
            }
        }

        self.side_to_move = them;
        self.key ^= KEYS.black_to_move;
        if us == Color::Black {
            self.fullmove_number = self.fullmove_number.wrapping_add(1);
        }
        self.history.push(Undo { captured, ..undo });

        if self.is_square_attacked(self.king_square(us), them) {
            self.unmake_move();
            return false;
        }
        debug_assert_eq!(self.check_invariants(), Ok(()), "after make_move {m}");
        true
    }

    /// Takes back the last move made with [`Position::make_move`].
    ///
    /// # Panics
    /// If no move has been made.
    pub fn unmake_move(&mut self) {
        let undo = self
            .history
            .pop()
            .expect("unmake_move: no move to take back");
        let m = undo.mv;
        let (from, to) = (m.from(), m.to());
        let us = !self.side_to_move;
        self.side_to_move = us;
        if us == Color::Black {
            self.fullmove_number = self.fullmove_number.wrapping_sub(1);
        }

        if m.is_castle() {
            let (rook_from, rook_to) = castling_rook_move(to);
            self.move_piece(rook_to, rook_from);
        }
        match m.promotion_piece() {
            Some(_) => {
                self.remove_piece(to);
                self.put_piece(from, Piece::new(us, PieceType::Pawn));
            }
            None => self.move_piece(to, from),
        }
        if let Some(captured) = undo.captured {
            let sq = if m.is_en_passant() {
                en_passant_victim(to, us)
            } else {
                to
            };
            self.put_piece(sq, captured);
        }

        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.key = undo.key;
        debug_assert_eq!(self.check_invariants(), Ok(()), "after unmake_move {m}");
    }
}

/// The square of the pawn captured en passant: one rank behind the capturer's `to` square.
fn en_passant_victim(to: Square, capturer: Color) -> Square {
    match capturer {
        Color::White => to.offset(-8),
        Color::Black => to.offset(8),
    }
}

/// The rook's from and to squares for a castling move whose king lands on `king_to`.
fn castling_rook_move(king_to: Square) -> (Square, Square) {
    match king_to {
        Square::G1 => (Square::H1, Square::F1),
        Square::C1 => (Square::A1, Square::D1),
        Square::G8 => (Square::H8, Square::F8),
        Square::C8 => (Square::A8, Square::D8),
        _ => unreachable!("castling king can't land on {king_to}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::{self, MoveList};

    const SUITE: [&str; 7] = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ];

    /// Walks every legal line to `depth`, checking each position reached along the way.
    /// Returns the number of leaf nodes, so the walk can be checked against perft.
    fn walk(pos: &mut Position, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let mut moves = MoveList::new();
        movegen::generate(pos, &mut moves);
        let mut leaves = 0;
        for &m in moves.iter() {
            let before = pos.clone();
            if !pos.make_move(m) {
                assert_eq!(*pos, before, "rejected move {m} changed the position");
                assert_eq!(
                    pos.history, before.history,
                    "rejected move {m} left history"
                );
                continue;
            }
            assert_eq!(
                pos.check_invariants(),
                Ok(()),
                "after {m} in {}",
                before.to_fen()
            );
            // The incrementally updated position must match one built from scratch, which
            // also checks make_move and FEN parsing set the en passant square the same way.
            let rebuilt = Position::from_fen(&pos.to_fen())
                .unwrap_or_else(|e| panic!("{} after {m}: {e}", pos.to_fen()));
            assert_eq!(
                rebuilt.key(),
                pos.key(),
                "key after {m} in {}",
                before.to_fen()
            );
            leaves += walk(pos, depth - 1);
            pos.unmake_move();
            assert_eq!(*pos, before, "unmake {m} in {}", before.to_fen());
            assert_eq!(pos.history, before.history, "history after unmake {m}");
        }
        leaves
    }

    #[test]
    fn make_and_unmake_keep_the_position_consistent() {
        let expected_leaves = [8_902, 97_862, 2_812, 9_467, 9_467, 62_379, 89_890];
        for (fen, expected) in SUITE.into_iter().zip(expected_leaves) {
            let mut pos = Position::from_fen(fen).unwrap();
            assert_eq!(walk(&mut pos, 3), expected, "{fen}");
        }
    }

    #[test]
    fn double_push_sets_en_passant_only_when_capturable() {
        let mut pos = Position::from_fen("4k3/8/8/8/5p2/8/4P3/4K3 w - - 0 1").unwrap();
        let push = movegen::find_legal_move(&mut pos, "e2e4").unwrap();
        assert!(pos.make_move(push));
        assert_eq!(pos.en_passant(), Some(Square::E3));
        assert_eq!(pos.to_fen(), "4k3/8/8/8/4Pp2/8/8/4K3 b - e3 0 1");

        let mut quiet = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let push = movegen::find_legal_move(&mut quiet, "e2e4").unwrap();
        assert!(quiet.make_move(push));
        assert_eq!(quiet.en_passant(), None);
    }

    #[test]
    fn moves_update_rights_clocks_and_counters() {
        let mut pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 5 9").unwrap();
        let rook_move = movegen::find_legal_move(&mut pos, "h1h8").unwrap();
        assert!(pos.make_move(rook_move));
        // White's rook left h1 and captured black's on h8: both kingside rights go.
        assert_eq!(pos.castling().to_string(), "Qq");
        assert_eq!((pos.halfmove_clock(), pos.fullmove_number()), (0, 9));
        let king_move = movegen::find_legal_move(&mut pos, "e8d7").unwrap();
        assert!(pos.make_move(king_move));
        assert_eq!(pos.castling().to_string(), "Q");
        assert_eq!((pos.halfmove_clock(), pos.fullmove_number()), (1, 10));
        pos.unmake_move();
        pos.unmake_move();
        assert_eq!(pos.to_fen(), "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 5 9");
    }

    fn play(pos: &mut Position, moves: &str) {
        for text in moves.split_whitespace() {
            let m = movegen::find_legal_move(pos, text).unwrap_or_else(|| panic!("{text}"));
            assert!(pos.make_move(m));
        }
    }

    #[test]
    fn transpositions_have_equal_keys() {
        let mut a = Position::startpos();
        play(&mut a, "g1f3 g8f6 b1c3 b8c6");
        let mut b = Position::startpos();
        play(&mut b, "b1c3 b8c6 g1f3 g8f6");
        assert_eq!(a.key(), b.key());
        assert_eq!(a, b);
        // The same rights lost by different rook moves: same position, same key. Losing a
        // right changes the key even though the pieces end up where they started.
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        let mut c = Position::from_fen(fen).unwrap();
        play(&mut c, "a1b1 a8b8 b1a1 b8a8");
        let mut d = Position::from_fen(fen).unwrap();
        play(&mut d, "a1c1 a8c8 c1a1 c8a8");
        assert_eq!(c.to_fen(), "r3k2r/8/8/8/8/8/8/R3K2R w Kk - 4 3");
        assert_eq!(c.key(), d.key());
        assert_ne!(c.key(), Position::from_fen(fen).unwrap().key());
    }

    #[test]
    fn promotion_capture_on_a_rook_square_removes_that_right() {
        let mut pos = Position::from_fen("r3k3/1P6/8/8/8/8/8/4K3 w q - 0 1").unwrap();
        play(&mut pos, "b7a8q");
        assert_eq!(pos.castling(), CastlingRights::NONE);
        assert_eq!(pos.to_fen(), "Q3k3/8/8/8/8/8/8/4K3 b - - 0 1");
        pos.unmake_move();
        assert_eq!(pos.to_fen(), "r3k3/1P6/8/8/8/8/8/4K3 w q - 0 1");
    }

    #[test]
    fn unmaking_every_move_returns_to_the_start() {
        let mut pos = Position::startpos();
        play(
            &mut pos,
            "e2e4 d7d5 e4d5 g8f6 f1b5 c7c6 d5c6 d8a5 c6b7 a5b5 b7a8q",
        );
        for _ in 0..11 {
            pos.unmake_move();
        }
        assert_eq!(pos, Position::startpos());
        assert!(pos.history.is_empty());
    }

    #[test]
    #[should_panic(expected = "no move to take back")]
    fn unmaking_past_the_start_panics() {
        Position::startpos().unmake_move();
    }

    #[test]
    fn illegal_moves_are_rejected_and_leave_no_trace() {
        // The white bishop on f2 is pinned to the king on e1 by the black bishop on h4, so
        // it may not leave the diagonal.
        let mut pos = Position::from_fen("4k3/8/8/8/7b/8/5B2/4K3 w - - 0 1").unwrap();
        let before = pos.clone();
        assert!(!pos.make_move(Move::quiet(Square::F2, Square::E3)));
        assert_eq!(pos, before);
        assert!(pos.history.is_empty());
    }

    #[test]
    fn repetition_needs_the_same_position_and_side_to_move() {
        let mut pos = Position::startpos();
        play(&mut pos, "g1f3 g8f6 f3g1");
        assert!(!pos.is_repetition());
        play(&mut pos, "f6g8");
        assert!(
            pos.is_repetition(),
            "back to the start position after four plies"
        );
        // A pawn move makes everything before it unrepeatable.
        play(&mut pos, "e2e4 e7e5 g1f3 g8f6 f3g1");
        assert!(!pos.is_repetition());
        play(&mut pos, "f6g8");
        assert!(pos.is_repetition());
    }

    #[test]
    fn repetition_includes_game_history_before_a_fen() {
        // Counters come from the FEN, but only moves played on this Position can repeat.
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 30 60").unwrap();
        play(&mut pos, "a1a2 e8d8 a2a1");
        assert!(!pos.is_repetition());
        play(&mut pos, "d8e8");
        assert!(pos.is_repetition());
    }

    #[test]
    fn insufficient_material() {
        for (fen, expected) in [
            ("4k3/8/8/8/8/8/8/4K3 w - - 0 1", true),
            ("4k3/8/8/8/8/8/8/3NK3 w - - 0 1", true),
            ("4k3/8/8/8/8/8/8/3bK3 w - - 0 1", true),
            ("4k3/8/8/8/8/8/8/2NNK3 w - - 0 1", false),
            ("4k3/8/8/8/8/8/8/2NbK3 w - - 0 1", false),
            ("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1", false),
            ("4k3/8/8/8/8/8/8/3RK3 w - - 0 1", false),
        ] {
            let pos = Position::from_fen(fen).unwrap();
            assert_eq!(pos.has_insufficient_material(), expected, "{fen}");
        }
    }
}
