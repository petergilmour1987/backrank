//! Reading and writing positions in Forsyth-Edwards Notation.

use super::Position;
use crate::types::{Bitboard, CastlingRights, Color, File, Piece, PieceType, Rank, Square};
use std::fmt;

/// Why a FEN string was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FenError {
    /// The number of space-separated fields was not 4, 5 or 6.
    FieldCount(usize),
    /// The piece placement field is malformed.
    Board(String),
    /// The side to move is not `w` or `b`.
    SideToMove(String),
    /// The castling field is not `-` or a set of `KQkq`.
    Castling(String),
    /// The en passant field is malformed or impossible in this position.
    EnPassant(String),
    /// A move counter is not a number in range.
    Clock(String),
    /// The FEN is well formed but describes a position that cannot occur in a game.
    Illegal(String),
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::FieldCount(n) => write!(f, "FEN must have 4 to 6 fields, found {n}"),
            FenError::Board(msg) => write!(f, "bad FEN piece placement: {msg}"),
            FenError::SideToMove(s) => write!(f, "bad FEN side to move {s:?}: expected w or b"),
            FenError::Castling(s) => write!(f, "bad FEN castling field {s:?}"),
            FenError::EnPassant(msg) => write!(f, "bad FEN en passant field: {msg}"),
            FenError::Clock(msg) => write!(f, "bad FEN move counter: {msg}"),
            FenError::Illegal(msg) => write!(f, "illegal position: {msg}"),
        }
    }
}

impl std::error::Error for FenError {}

impl Position {
    /// Parses a FEN string.
    ///
    /// The halfmove clock and fullmove number may be left off, as in EPD; they default to
    /// 0 and 1. A fullmove number of 0, which some tools write, is read as 1. An en passant
    /// square is dropped when no pawn of the side to move could capture onto it, so it
    /// will be missing from [`Position::to_fen`].
    pub fn from_fen(fen: &str) -> Result<Position, FenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if !(4..=6).contains(&fields.len()) {
            return Err(FenError::FieldCount(fields.len()));
        }

        let mut pos = Position::empty();
        parse_board(&mut pos, fields[0])?;
        pos.side_to_move = match fields[1] {
            "w" => Color::White,
            "b" => Color::Black,
            other => return Err(FenError::SideToMove(other.to_string())),
        };
        pos.castling = CastlingRights::from_fen(fields[2])
            .ok_or_else(|| FenError::Castling(fields[2].to_string()))?;
        let en_passant = match fields[3] {
            "-" => None,
            field => Some(
                field
                    .parse::<Square>()
                    .map_err(|_| FenError::EnPassant(format!("{field:?} is not a square or -")))?,
            ),
        };
        pos.halfmove_clock = parse_counter(fields.get(4), "halfmove clock", 0)?;
        pos.fullmove_number = parse_counter(fields.get(5), "fullmove number", 1)?.max(1);

        check_material(&pos)?;
        check_castling(&pos)?;
        let them = !pos.side_to_move;
        if pos.is_square_attacked(pos.king_square(them), pos.side_to_move) {
            return Err(FenError::Illegal(format!(
                "{them} is in check but it is {} to move",
                pos.side_to_move
            )));
        }
        pos.en_passant = match en_passant {
            Some(sq) => usable_en_passant(&pos, sq)?,
            None => None,
        };
        pos.key = pos.compute_key();
        debug_assert_eq!(pos.check_invariants(), Ok(()));
        Ok(pos)
    }

    /// The position as a six-field FEN string.
    pub fn to_fen(&self) -> String {
        let mut fen = String::with_capacity(90);
        for rank in Rank::ALL.into_iter().rev() {
            let mut empty = 0;
            for file in File::ALL {
                match self.piece_at(Square::new(file, rank)) {
                    Some(piece) => {
                        if empty > 0 {
                            fen.push_str(&empty.to_string());
                            empty = 0;
                        }
                        fen.push(piece.to_fen_char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                fen.push_str(&empty.to_string());
            }
            if rank != Rank::R1 {
                fen.push('/');
            }
        }
        let side = match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        };
        let en_passant = self
            .en_passant
            .map_or_else(|| "-".to_string(), |sq| sq.to_string());
        fen.push_str(&format!(
            " {side} {} {en_passant} {} {}",
            self.castling, self.halfmove_clock, self.fullmove_number
        ));
        fen
    }
}

/// Fills in the pieces from the first FEN field: ranks 8 down to 1, separated by `/`.
fn parse_board(pos: &mut Position, field: &str) -> Result<(), FenError> {
    let ranks: Vec<&str> = field.split('/').collect();
    if ranks.len() != 8 {
        return Err(FenError::Board(format!(
            "expected 8 ranks separated by /, found {}",
            ranks.len()
        )));
    }
    for (rank, text) in Rank::ALL.into_iter().rev().zip(ranks) {
        let rank_name = rank.to_char();
        let too_long = || FenError::Board(format!("rank {rank_name} has more than 8 squares"));
        let mut file = 0;
        for c in text.chars() {
            if let Some(run) = c.to_digit(10) {
                if !(1..=8).contains(&run) {
                    return Err(FenError::Board(format!(
                        "empty-square count {c} on rank {rank_name}"
                    )));
                }
                file += run as usize;
                if file > 8 {
                    return Err(too_long());
                }
            } else {
                let piece = Piece::from_fen_char(c).ok_or_else(|| {
                    FenError::Board(format!("unknown piece {c:?} on rank {rank_name}"))
                })?;
                let file_at = File::from_index(file).ok_or_else(too_long)?;
                pos.put_piece(Square::new(file_at, rank), piece);
                file += 1;
            }
        }
        if file != 8 {
            return Err(FenError::Board(format!(
                "rank {rank_name} has {file} squares, expected 8"
            )));
        }
    }
    Ok(())
}

fn parse_counter(field: Option<&&str>, name: &str, default: u16) -> Result<u16, FenError> {
    match field {
        None => Ok(default),
        Some(text) => text.parse().map_err(|_| {
            FenError::Clock(format!("{name} {text:?} is not a number from 0 to 65535"))
        }),
    }
}

/// One king each, no pawns on the back ranks, and no more pieces than a game can have.
fn check_material(pos: &Position) -> Result<(), FenError> {
    for color in Color::ALL {
        let kings = pos.pieces(Piece::new(color, PieceType::King)).count();
        if kings != 1 {
            return Err(FenError::Illegal(format!("{color} has {kings} kings")));
        }
        let pawns = pos.pieces(Piece::new(color, PieceType::Pawn));
        if pawns.count() > 8 {
            return Err(FenError::Illegal(format!("{color} has more than 8 pawns")));
        }
        if pos.occupancy(color).count() > 16 {
            return Err(FenError::Illegal(format!(
                "{color} has more than 16 pieces"
            )));
        }
        // Every piece beyond the starting set must be a promoted pawn. This also keeps
        // positions within the 256 moves a move list can hold.
        let count = |kind| pos.pieces(Piece::new(color, kind)).count();
        let promoted = count(PieceType::Queen).saturating_sub(1)
            + count(PieceType::Rook).saturating_sub(2)
            + count(PieceType::Bishop).saturating_sub(2)
            + count(PieceType::Knight).saturating_sub(2);
        if pawns.count() + promoted > 8 {
            return Err(FenError::Illegal(format!(
                "{color} has more extra pieces than pawns that could have promoted"
            )));
        }
        let back_ranks = Bitboard::rank(Rank::R1) | Bitboard::rank(Rank::R8);
        if let Some(sq) = (pawns & back_ranks).lsb() {
            return Err(FenError::Illegal(format!("{color} pawn on {sq}")));
        }
    }
    Ok(())
}

/// Each castling right needs its king and rook still on their starting squares.
fn check_castling(pos: &Position) -> Result<(), FenError> {
    const RULES: [(CastlingRights, char, Color, Square, Square); 4] = [
        (
            CastlingRights::WHITE_KINGSIDE,
            'K',
            Color::White,
            Square::E1,
            Square::H1,
        ),
        (
            CastlingRights::WHITE_QUEENSIDE,
            'Q',
            Color::White,
            Square::E1,
            Square::A1,
        ),
        (
            CastlingRights::BLACK_KINGSIDE,
            'k',
            Color::Black,
            Square::E8,
            Square::H8,
        ),
        (
            CastlingRights::BLACK_QUEENSIDE,
            'q',
            Color::Black,
            Square::E8,
            Square::A8,
        ),
    ];
    for (right, letter, color, king, rook) in RULES {
        if pos.castling.contains(right)
            && (pos.piece_at(king) != Some(Piece::new(color, PieceType::King))
                || pos.piece_at(rook) != Some(Piece::new(color, PieceType::Rook)))
        {
            return Err(FenError::Illegal(format!(
                "castling right {letter} needs the {color} king on {king} and a rook on {rook}"
            )));
        }
    }
    Ok(())
}

/// Checks the en passant square is consistent with a pawn having just advanced two
/// squares, and returns it only if the side to move has a pawn that could capture there.
fn usable_en_passant(pos: &Position, sq: Square) -> Result<Option<Square>, FenError> {
    let us = pos.side_to_move;
    let them = !us;
    // Ranks of the en passant square, the pawn that just moved, and where it came from.
    let (ep_rank, pawn_rank, origin_rank) = match us {
        Color::White => (Rank::R6, Rank::R5, Rank::R7),
        Color::Black => (Rank::R3, Rank::R4, Rank::R2),
    };
    if sq.rank() != ep_rank {
        return Err(FenError::EnPassant(format!(
            "{sq} is not on rank {} with {us} to move",
            ep_rank.to_char()
        )));
    }
    let pushed = Square::new(sq.file(), pawn_rank);
    let origin = Square::new(sq.file(), origin_rank);
    if pos.piece_at(pushed) != Some(Piece::new(them, PieceType::Pawn)) {
        return Err(FenError::EnPassant(format!(
            "{sq} needs a {them} pawn on {pushed} that has just moved two squares"
        )));
    }
    if pos.piece_at(sq).is_some() || pos.piece_at(origin).is_some() {
        return Err(FenError::EnPassant(format!(
            "{sq} and {origin} must be empty after a two-square pawn move"
        )));
    }
    Ok(pos.can_capture_en_passant(sq, us).then_some(sq))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The perft suite positions from docs/plan-v1.md §7.2, all with six fields.
    const PERFT_FENS: [&str; 7] = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ];

    fn parse(fen: &str) -> Position {
        Position::from_fen(fen).unwrap_or_else(|e| panic!("{fen}: {e}"))
    }

    #[test]
    fn perft_positions_round_trip() {
        for fen in PERFT_FENS {
            let pos = parse(fen);
            assert_eq!(pos.to_fen(), fen);
            assert_eq!(pos.check_invariants(), Ok(()));
            assert_eq!(parse(&pos.to_fen()), pos);
        }
    }

    #[test]
    fn start_position() {
        let pos = Position::startpos();
        assert_eq!(pos.to_fen(), Position::STARTPOS_FEN);
        assert_eq!(pos.side_to_move(), Color::White);
        assert_eq!(pos.castling(), CastlingRights::ALL);
        assert_eq!(pos.en_passant(), None);
        assert_eq!((pos.halfmove_clock(), pos.fullmove_number()), (0, 1));
        assert_eq!(pos.occupancy(Color::White).count(), 16);
        assert_eq!(pos.occupancy(Color::Black).count(), 16);
        assert_eq!(pos.occupied().count(), 32);
        assert_eq!(pos.king_square(Color::White), Square::E1);
        assert_eq!(pos.king_square(Color::Black), Square::E8);
        assert_eq!(
            pos.piece_at(Square::D8),
            Some(Piece::new(Color::Black, PieceType::Queen))
        );
        assert_eq!(pos.piece_at(Square::E4), None);
        assert_eq!(
            pos.pieces(Piece::new(Color::White, PieceType::Pawn)),
            Bitboard::rank(Rank::R2)
        );
    }

    #[test]
    fn move_counters_are_optional() {
        // Kiwipete as chessprogramming.org writes it, without counters.
        let pos = parse("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
        assert_eq!(pos.to_fen(), PERFT_FENS[1]);
        let five = parse("8/8/8/8/8/8/8/K6k b - - 37");
        assert_eq!(five.to_fen(), "8/8/8/8/8/8/8/K6k b - - 37 1");
    }

    #[test]
    fn tolerates_extra_whitespace_and_fullmove_zero() {
        let pos = parse("  8/8/8/8/8/8/8/K6k   w  -  -  3  0 ");
        assert_eq!(pos.to_fen(), "8/8/8/8/8/8/8/K6k w - - 3 1");
    }

    #[test]
    fn keeps_en_passant_only_when_a_capture_is_possible() {
        // After 1. e4: no black pawn can take on e3, so the square is dropped.
        let after_e4 = parse("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        let without = parse("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        assert_eq!(after_e4.en_passant(), None);
        assert_eq!(after_e4.to_fen(), without.to_fen());
        assert_eq!(after_e4.key(), without.key());

        // White pawn on e5 next to the black pawn that just moved f7-f5.
        let capturable = "rnbqkbnr/ppppp1pp/8/4Pp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3";
        let pos = parse(capturable);
        assert_eq!(pos.en_passant(), Some(Square::F6));
        assert_eq!(pos.to_fen(), capturable);
        let no_ep = parse("rnbqkbnr/ppppp1pp/8/4Pp2/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3");
        assert_ne!(pos.key(), no_ep.key());
    }

    #[test]
    fn en_passant_for_black_and_on_edge_files() {
        // Black to move after e2-e4, with a black pawn on f4 that can take.
        let black = "4k3/8/8/8/4Pp2/8/8/4K3 b - e3 0 1";
        assert_eq!(parse(black).en_passant(), Some(Square::E3));
        assert_eq!(parse(black).to_fen(), black);
        // Capturers on the a- and h-file edges.
        let a_file = "4k3/8/8/pP6/8/8/8/4K3 w - a6 0 1";
        assert_eq!(parse(a_file).en_passant(), Some(Square::A6));
        let h_file = "4k3/8/8/6Pp/8/8/8/4K3 w - h6 0 1";
        assert_eq!(parse(h_file).en_passant(), Some(Square::H6));
        // A pawn on the far side of the board is not a capturer.
        let far = parse("4k3/8/8/P6p/8/8/8/4K3 w - h6 0 1");
        assert_eq!(far.en_passant(), None);
        // Black to move with no capturer: dropped, and the key matches "-".
        let dropped = parse("4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1");
        assert_eq!(
            dropped.key(),
            parse("4k3/8/8/8/4P3/8/8/4K3 b - - 0 1").key()
        );
    }

    #[test]
    fn castling_letter_order_does_not_change_the_key() {
        let canonical = parse("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let shuffled = parse("r3k2r/8/8/8/8/8/8/R3K2R w qkQK - 0 1");
        assert_eq!(canonical.key(), shuffled.key());
        assert_eq!(shuffled.to_fen(), canonical.to_fen());
    }

    #[test]
    fn side_and_castling_change_the_key() {
        let white = parse("8/8/8/8/8/8/8/K6k w - - 0 1");
        let black = parse("8/8/8/8/8/8/8/K6k b - - 0 1");
        assert_ne!(white.key(), black.key());
        let start = Position::startpos();
        let no_castling = parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1");
        assert_ne!(start.key(), no_castling.key());
        // Counters don't affect the key.
        let later = parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 12 40");
        assert_eq!(start.key(), later.key());
    }

    #[test]
    fn rejects_malformed_fens() {
        type IsExpected = fn(&FenError) -> bool;
        let cases: [(&str, IsExpected); 16] = [
            ("", |e| matches!(e, FenError::FieldCount(0))),
            ("8/8/8/8/8/8/8/K6k w -", |e| {
                matches!(e, FenError::FieldCount(3))
            }),
            ("8/8/8/8/8/8/8/K6k w - - 0 1 x", |e| {
                matches!(e, FenError::FieldCount(7))
            }),
            ("8/8/8/8/8/8/K6k w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K6k/8 w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K5k w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K7k w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K6kp w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K6x w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K06k w - - 0 1", |e| {
                matches!(e, FenError::Board(_))
            }),
            ("8/8/8/8/8/8/8/K6k x - - 0 1", |e| {
                matches!(e, FenError::SideToMove(_))
            }),
            ("8/8/8/8/8/8/8/K6k w KK - 0 1", |e| {
                matches!(e, FenError::Castling(_))
            }),
            ("8/8/8/8/8/8/8/K6k w - e9 0 1", |e| {
                matches!(e, FenError::EnPassant(_))
            }),
            ("8/8/8/8/8/8/8/K6k w - - x 1", |e| {
                matches!(e, FenError::Clock(_))
            }),
            ("8/8/8/8/8/8/8/K6k w - - 0 70000", |e| {
                matches!(e, FenError::Clock(_))
            }),
            ("8/8/8/8/8/8/8/K6k w - - -1 1", |e| {
                matches!(e, FenError::Clock(_))
            }),
        ];
        for (fen, is_expected) in cases {
            match Position::from_fen(fen) {
                Ok(pos) => panic!("{fen:?} parsed as {}", pos.to_fen()),
                Err(e) => assert!(is_expected(&e), "{fen:?} gave {e:?}"),
            }
        }
    }

    #[test]
    fn rejects_impossible_positions() {
        for fen in [
            "8/8/8/8/8/8/8/K7 w - - 0 1",                 // no black king
            "8/8/8/8/8/8/8/KK5k w - - 0 1",               // two white kings
            "P7/8/8/8/8/8/8/K6k w - - 0 1",               // pawn on rank 8
            "8/8/8/8/8/8/8/K5pk w - - 0 1",               // pawn on rank 1
            "4k3/pppppppp/p7/8/8/8/8/4K3 w - - 0 1",      // nine black pawns
            "4k3/8/8/8/8/8/8/4K3 w K - 0 1",              // K right, no rook
            "4k3/8/8/8/8/8/8/R3K3 w K - 0 1",             // K right, rook on a1
            "4k2r/8/8/8/8/8/8/4K3 w q - 0 1",             // q right, rook on h8
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq e3 0 1",      // ep rank wrong for white
            "4k3/8/8/4P3/8/8/8/4K3 w - e6 0 1",           // ep: no black pawn on e5
            "4k3/4p3/8/4p3/3P4/8/8/4K3 w - e6 0 1",       // ep: e7 not empty
            "4k3/8/8/8/4p3/8/8/4K3 b - e3 0 1",           // ep: e4 pawn is black
            "4k3/8/8/8/3pP3/8/4P3/4K3 b - e3 0 1",        // ep: e2 not empty
            "r3k3/8/8/8/8/8/8/4K3 w Q - 0 1",             // Q right, no rook on a1
            "4k2R/8/8/8/8/8/8/4K3 w k - 0 1",             // k right, white rook on h8
            "r3k2r/8/8/8/8/8/8/R2K3R w K - 0 1",          // K right, king not on e1
            "4k3/8/8/8/8/N7/NNNNNNNN/NNNNKNNN w - - 0 1", // 17 white pieces
            "QQQQQQQQ/QQQQQQQ1/8/8/8/8/8/k3K3 w - - 0 1", // 15 queens
            "4k3/8/8/8/8/8/PPPPPPPP/QQ2K3 w - - 0 1",     // 8 pawns and an extra queen
            "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1",            // black in check, white to move
            "3kK3/8/8/8/8/8/8/8 w - - 0 1",               // kings next to each other
        ] {
            let result = Position::from_fen(fen);
            assert!(
                matches!(result, Err(FenError::Illegal(_) | FenError::EnPassant(_))),
                "{fen:?} gave {result:?}"
            );
        }
    }

    #[test]
    fn invariant_check_catches_corruption() {
        let mut bad_key = Position::startpos();
        bad_key.key ^= 1;
        assert!(bad_key.check_invariants().unwrap_err().contains("key"));

        let mut bad_mailbox = Position::startpos();
        bad_mailbox.mailbox[Square::E4.index()] = Some(Piece::new(Color::White, PieceType::Knight));
        assert!(
            bad_mailbox
                .check_invariants()
                .unwrap_err()
                .starts_with("e4")
        );

        let mut bad_occupancy = Position::startpos();
        bad_occupancy.colors[Color::White.index()] = Bitboard::EMPTY;
        assert!(bad_occupancy.check_invariants().is_err());
    }

    #[test]
    fn displays_board_fen_and_key() {
        let shown = Position::startpos().to_string();
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines[0], "8  r n b q k b n r");
        assert_eq!(lines[7], "1  R N B Q K B N R");
        assert_eq!(lines[8], "   a b c d e f g h");
        assert_eq!(lines[10], format!("FEN: {}", Position::STARTPOS_FEN));
        assert!(lines[11].starts_with("Key: "));
    }

    #[test]
    fn errors_have_readable_messages() {
        let err = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w K - 0 1").unwrap_err();
        assert_eq!(
            err.to_string(),
            "illegal position: castling right K needs the white king on e1 and a rook on h1"
        );
    }
}
