//! The board position.

mod fen;
mod zobrist;

pub use fen::FenError;

use crate::types::{Bitboard, CastlingRights, Color, File, Piece, PieceType, Rank, Square};
use std::fmt;
use zobrist::KEYS;

/// A chess position: where the pieces are, whose move it is, castling rights, the en
/// passant square, the move clocks, and the position's Zobrist key.
///
/// Pieces are stored twice, as bitboards (for move generation) and as a square-to-piece
/// array (for "what is on this square?"). The two always agree.
#[derive(Clone, PartialEq, Eq)]
pub struct Position {
    /// One bitboard per piece, indexed by [`Piece::index`].
    pieces: [Bitboard; 12],
    /// All pieces of each colour, indexed by [`Color::index`].
    colors: [Bitboard; 2],
    /// What is on each square, indexed by [`Square::index`].
    mailbox: [Option<Piece>; 64],
    side_to_move: Color,
    castling: CastlingRights,
    /// Only set when the side to move has a pawn that could capture en passant, so
    /// positions that FIDE counts as the same also have the same key.
    en_passant: Option<Square>,
    /// Half-moves since the last capture or pawn move, for the fifty-move rule.
    halfmove_clock: u16,
    /// Starts at 1 and goes up after each black move.
    fullmove_number: u16,
    key: u64,
}

impl Position {
    /// The standard starting position.
    pub const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    pub fn startpos() -> Position {
        Position::from_fen(Self::STARTPOS_FEN).expect("the start position FEN is valid")
    }

    /// A board with no pieces, white to move. Only used while building a position.
    fn empty() -> Position {
        Position {
            pieces: [Bitboard::EMPTY; 12],
            colors: [Bitboard::EMPTY; 2],
            mailbox: [None; 64],
            side_to_move: Color::White,
            castling: CastlingRights::NONE,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            key: 0,
        }
    }

    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        self.mailbox[sq.index()]
    }

    /// The squares holding `piece`.
    pub fn pieces(&self, piece: Piece) -> Bitboard {
        self.pieces[piece.index()]
    }

    /// The squares holding any piece of `color`.
    pub fn occupancy(&self, color: Color) -> Bitboard {
        self.colors[color.index()]
    }

    /// The squares holding any piece.
    pub fn occupied(&self) -> Bitboard {
        self.colors[0] | self.colors[1]
    }

    pub fn king_square(&self, color: Color) -> Square {
        self.pieces(Piece::new(color, PieceType::King))
            .lsb()
            .expect("a position always has one king of each colour")
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn castling(&self) -> CastlingRights {
        self.castling
    }

    /// The square a pawn can capture onto en passant, if any capture is possible.
    pub fn en_passant(&self) -> Option<Square> {
        self.en_passant
    }

    pub fn halfmove_clock(&self) -> u16 {
        self.halfmove_clock
    }

    pub fn fullmove_number(&self) -> u16 {
        self.fullmove_number
    }

    /// The Zobrist key: equal positions have equal keys.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Places `piece` on the empty square `sq`, keeping the key up to date.
    fn put_piece(&mut self, sq: Square, piece: Piece) {
        debug_assert!(self.mailbox[sq.index()].is_none(), "{sq} is occupied");
        let bb = Bitboard::from_square(sq);
        self.pieces[piece.index()] |= bb;
        self.colors[piece.color().index()] |= bb;
        self.mailbox[sq.index()] = Some(piece);
        self.key ^= KEYS.pieces[piece.index()][sq.index()];
    }

    /// The key computed from scratch, to check the incrementally updated one.
    fn compute_key(&self) -> u64 {
        let mut key = 0;
        for piece in Piece::all() {
            for sq in self.pieces(piece) {
                key ^= KEYS.pieces[piece.index()][sq.index()];
            }
        }
        key ^= KEYS.castling[self.castling.index()];
        if let Some(sq) = self.en_passant {
            key ^= KEYS.en_passant[sq.file().index()];
        }
        if self.side_to_move == Color::Black {
            key ^= KEYS.black_to_move;
        }
        key
    }

    /// Checks that the redundant parts of the position agree with each other: bitboards
    /// with the mailbox, colour boards with piece boards, the key with a fresh computation,
    /// one king per side, and the en passant square on the right rank.
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        for sq in Square::all() {
            let holders: Vec<Piece> = Piece::all()
                .filter(|&p| self.pieces(p).contains(sq))
                .collect();
            match (self.mailbox[sq.index()], holders.as_slice()) {
                (None, []) => {}
                (Some(piece), [holder]) if piece == *holder => {}
                (mailbox, bitboards) => {
                    return Err(format!(
                        "{sq}: mailbox has {mailbox:?} but piece bitboards have {bitboards:?}"
                    ));
                }
            }
        }
        for color in Color::ALL {
            let union = PieceType::ALL
                .into_iter()
                .fold(Bitboard::EMPTY, |acc, kind| {
                    acc | self.pieces(Piece::new(color, kind))
                });
            if union != self.occupancy(color) {
                return Err(format!(
                    "{color} occupancy disagrees with its piece bitboards"
                ));
            }
            let kings = self.pieces(Piece::new(color, PieceType::King)).count();
            if kings != 1 {
                return Err(format!("{color} has {kings} kings"));
            }
        }
        if let Some(sq) = self.en_passant {
            let expected = match self.side_to_move {
                Color::White => Rank::R6,
                Color::Black => Rank::R3,
            };
            if sq.rank() != expected {
                return Err(format!("en passant square {sq} is on the wrong rank"));
            }
        }
        let fresh = self.compute_key();
        if self.key != fresh {
            return Err(format!(
                "key {:#018x} differs from recomputed {fresh:#018x}",
                self.key
            ));
        }
        Ok(())
    }
}

/// An ASCII board (rank 8 at the top, white pieces in uppercase), then the FEN and key.
impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in Rank::ALL.into_iter().rev() {
            write!(f, "{} ", rank.to_char())?;
            for file in File::ALL {
                let c = self
                    .piece_at(Square::new(file, rank))
                    .map_or('.', Piece::to_fen_char);
                write!(f, " {c}")?;
            }
            writeln!(f)?;
        }
        writeln!(f, "   a b c d e f g h")?;
        writeln!(f)?;
        writeln!(f, "FEN: {}", self.to_fen())?;
        write!(f, "Key: {:016x}", self.key)
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
