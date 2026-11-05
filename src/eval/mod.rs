//! Static evaluation: how good a position looks without searching.
//!
//! Scores are in centipawns (a pawn is 100) from the side to move's point of view, as
//! negamax needs. This is Tomasz Michniewski's Simplified Evaluation Function, from
//! chessprogramming.org: material plus a piece-square table for each piece, with a
//! different king table in the endgame.

use crate::board::Position;
use crate::types::{Color, Piece, PieceType, Square};

/// Material values in centipawns, indexed by [`PieceType::index`]. Kings are never
/// captured, so they count for nothing.
pub const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

// Piece-square tables: the bonus for a piece on each square, from white's point of view
// and laid out as a board is printed, with a8 first and h1 last. Black uses them mirrored.

#[rustfmt::skip]
const PAWN: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
    5, 5, 10, 25, 25, 10, 5, 5,
    0, 0, 0, 20, 20, 0, 0, 0,
    5, -5, -10, 0, 0, -10, -5, 5,
    5, 10, 10, -20, -20, 10, 10, 5,
    0, 0, 0, 0, 0, 0, 0, 0,
];

#[rustfmt::skip]
const KNIGHT: [i32; 64] = [
    -50, -40, -30, -30, -30, -30, -40, -50,
    -40, -20, 0, 0, 0, 0, -20, -40,
    -30, 0, 10, 15, 15, 10, 0, -30,
    -30, 5, 15, 20, 20, 15, 5, -30,
    -30, 0, 15, 20, 20, 15, 0, -30,
    -30, 5, 10, 15, 15, 10, 5, -30,
    -40, -20, 0, 5, 5, 0, -20, -40,
    -50, -40, -30, -30, -30, -30, -40, -50,
];

#[rustfmt::skip]
const BISHOP: [i32; 64] = [
    -20, -10, -10, -10, -10, -10, -10, -20,
    -10, 0, 0, 0, 0, 0, 0, -10,
    -10, 0, 5, 10, 10, 5, 0, -10,
    -10, 5, 5, 10, 10, 5, 5, -10,
    -10, 0, 10, 10, 10, 10, 0, -10,
    -10, 10, 10, 10, 10, 10, 10, -10,
    -10, 5, 0, 0, 0, 0, 5, -10,
    -20, -10, -10, -10, -10, -10, -10, -20,
];

#[rustfmt::skip]
const ROOK: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0,
    5, 10, 10, 10, 10, 10, 10, 5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    -5, 0, 0, 0, 0, 0, 0, -5,
    0, 0, 0, 5, 5, 0, 0, 0,
];

#[rustfmt::skip]
const QUEEN: [i32; 64] = [
    -20, -10, -10, -5, -5, -10, -10, -20,
    -10, 0, 0, 0, 0, 0, 0, -10,
    -10, 0, 5, 5, 5, 5, 0, -10,
    -5, 0, 5, 5, 5, 5, 0, -5,
    0, 0, 5, 5, 5, 5, 0, -5,
    -10, 5, 5, 5, 5, 5, 0, -10,
    -10, 0, 5, 0, 0, 0, 0, -10,
    -20, -10, -10, -5, -5, -10, -10, -20,
];

#[rustfmt::skip]
const KING_MG: [i32; 64] = [
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -30, -40, -40, -50, -50, -40, -40, -30,
    -20, -30, -30, -40, -40, -30, -30, -20,
    -10, -20, -20, -20, -20, -20, -20, -10,
    20, 20, 0, 0, 0, 0, 20, 20,
    20, 30, 10, 0, 0, 10, 30, 20,
];

#[rustfmt::skip]
const KING_EG: [i32; 64] = [
    -50, -40, -30, -20, -20, -30, -40, -50,
    -30, -20, -10, 0, 0, -10, -20, -30,
    -30, -10, 20, 30, 30, 20, -10, -30,
    -30, -10, 30, 40, 40, 30, -10, -30,
    -30, -10, 30, 40, 40, 30, -10, -30,
    -30, -10, 20, 30, 30, 20, -10, -30,
    -30, -30, 0, 0, 0, 0, -30, -30,
    -50, -30, -30, -30, -30, -30, -30, -50,
];

/// The table's bonus for a piece of `color` on `sq`.
fn square_bonus(table: &[i32; 64], color: Color, sq: Square) -> i32 {
    // Tables start at a8, while square indices start at a1. Flipping the rank (XOR 56)
    // converts one to the other for white; black's view is the mirror image of that.
    match color {
        Color::White => table[sq.index() ^ 56],
        Color::Black => table[sq.index()],
    }
}

/// The endgame, where the king should head for the centre: every side that has a queen
/// has nothing else but at most one minor piece (which includes neither side having one).
fn is_endgame(pos: &Position) -> bool {
    Color::ALL.into_iter().all(|color| {
        let count = |kind| pos.pieces(Piece::new(color, kind)).count();
        let queens = count(PieceType::Queen);
        queens == 0
            || (queens == 1
                && count(PieceType::Rook) == 0
                && count(PieceType::Knight) + count(PieceType::Bishop) <= 1)
    })
}

/// The position's value for the side to move.
pub fn evaluate(pos: &Position) -> i32 {
    let king_table = if is_endgame(pos) { &KING_EG } else { &KING_MG };
    let mut white_minus_black = 0;
    for color in Color::ALL {
        let mut own = 0;
        for kind in PieceType::ALL {
            let table = match kind {
                PieceType::Pawn => &PAWN,
                PieceType::Knight => &KNIGHT,
                PieceType::Bishop => &BISHOP,
                PieceType::Rook => &ROOK,
                PieceType::Queen => &QUEEN,
                PieceType::King => king_table,
            };
            for sq in pos.pieces(Piece::new(color, kind)) {
                own += PIECE_VALUES[kind.index()] + square_bonus(table, color, sq);
            }
        }
        white_minus_black += match color {
            Color::White => own,
            Color::Black => -own,
        };
    }
    match pos.side_to_move() {
        Color::White => white_minus_black,
        Color::Black => -white_minus_black,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(fen: &str) -> i32 {
        evaluate(&Position::from_fen(fen).unwrap())
    }

    /// The same position with the board flipped top to bottom and the colours swapped.
    fn mirrored(fen: &str) -> String {
        let swap_case = |c: char| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c.to_ascii_uppercase()
            }
        };
        let fields: Vec<&str> = fen.split_whitespace().collect();
        let board: Vec<String> = fields[0]
            .split('/')
            .rev()
            .map(|rank| rank.chars().map(swap_case).collect())
            .collect();
        let side = if fields[1] == "w" { "b" } else { "w" };
        let castling: String = if fields[2] == "-" {
            "-".to_string()
        } else {
            fields[2].chars().map(swap_case).collect()
        };
        format!("{} {side} {castling} - 0 1", board.join("/"))
    }

    #[test]
    fn start_position_is_level() {
        assert_eq!(evaluate(&Position::startpos()), 0);
    }

    #[test]
    fn scores_are_from_the_side_to_moves_view() {
        let white_to_move = eval("4k3/8/8/8/8/8/8/3QK3 w - - 0 1");
        let black_to_move = eval("4k3/8/8/8/8/8/8/3QK3 b - - 0 1");
        assert!(white_to_move > 800);
        assert_eq!(black_to_move, -white_to_move);
    }

    #[test]
    fn mirrored_positions_score_the_same() {
        for fen in [
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        ] {
            assert_eq!(eval(fen), eval(&mirrored(fen)), "{fen}");
        }
    }

    #[test]
    fn tables_are_read_from_the_right_side_of_the_board() {
        // A white pawn on the seventh rank is worth more than on the second, and black's
        // pawn is the mirror image.
        assert_eq!(square_bonus(&PAWN, Color::White, Square::A7), 50);
        assert_eq!(square_bonus(&PAWN, Color::White, Square::E2), -20);
        assert_eq!(square_bonus(&PAWN, Color::Black, Square::A2), 50);
        assert_eq!(square_bonus(&PAWN, Color::Black, Square::E7), -20);
        assert_eq!(square_bonus(&KNIGHT, Color::White, Square::D4), 20);
        assert_eq!(square_bonus(&KNIGHT, Color::White, Square::A1), -50);
        assert_eq!(square_bonus(&KING_MG, Color::White, Square::G1), 30);
        assert_eq!(square_bonus(&KING_MG, Color::Black, Square::G8), 30);
        assert_eq!(square_bonus(&KING_EG, Color::White, Square::E4), 40);
        // The rook likes the seventh rank, and the queen table isn't left-right symmetric.
        assert_eq!(square_bonus(&ROOK, Color::White, Square::D7), 10);
        assert_eq!(square_bonus(&QUEEN, Color::White, Square::A4), 0);
        assert_eq!(square_bonus(&QUEEN, Color::White, Square::H4), -5);
    }

    #[test]
    fn recognises_the_endgame() {
        let endgame = |fen| is_endgame(&Position::from_fen(fen).unwrap());
        assert!(!endgame(Position::STARTPOS_FEN));
        // No queens at all.
        assert!(endgame("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1"));
        // Queen and one minor piece against no queen.
        assert!(endgame("4k3/8/8/8/8/8/8/2BQK3 w - - 0 1"));
        // Queen and rook is too much.
        assert!(!endgame("4k3/8/8/8/8/8/8/2RQK3 w - - 0 1"));
        // Queen and two minor pieces is too much, even with black bare.
        assert!(!endgame("4k3/8/8/8/8/8/8/1NBQK3 w - - 0 1"));
    }

    #[test]
    fn the_king_changes_its_mind_in_the_endgame() {
        // Without queens the king scores better in the centre than in the corner.
        let centre = eval("8/5k2/8/8/4K3/8/P7/8 w - - 0 1");
        let corner = eval("8/5k2/8/8/8/8/P7/7K w - - 0 1");
        assert!(centre > corner, "{centre} {corner}");
        // With queens on, it scores better tucked away.
        let tucked = eval("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ1RK1 w kq - 0 1");
        let central = eval("rnbqkbnr/pppppppp/8/8/4K3/8/PPPPPPPP/RNBQ1R2 w kq - 0 1");
        assert!(tucked > central, "{tucked} {central}");
    }
}
