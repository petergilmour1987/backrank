//! Building a position from the arguments of a UCI `position` command.

use crate::board::{FenError, Position};
use crate::movegen;
use std::fmt;

/// Why a position description was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionError {
    Fen(FenError),
    /// A move in the `moves` list is not legal where it is played.
    IllegalMove(String),
    /// The tokens don't follow `[startpos | fen <FEN>] [moves <move>...]`.
    Syntax(String),
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionError::Fen(e) => write!(f, "{e}"),
            PositionError::IllegalMove(m) => write!(f, "illegal move {m}"),
            PositionError::Syntax(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PositionError {}

/// Builds a position from the arguments of a UCI `position` command:
/// `[startpos | fen <FEN>] [moves <move>...]`. With no starting position given, the
/// standard start position is used.
pub fn parse_position(tokens: &[&str]) -> Result<Position, PositionError> {
    let moves_at = tokens.iter().position(|&t| t == "moves");
    let (setup, moves) = match moves_at {
        Some(i) => (&tokens[..i], &tokens[i + 1..]),
        None => (tokens, &[][..]),
    };
    let mut pos = match setup {
        [] | ["startpos"] => Position::startpos(),
        ["fen", fields @ ..] => {
            Position::from_fen(&fields.join(" ")).map_err(PositionError::Fen)?
        }
        _ => {
            return Err(PositionError::Syntax(format!(
                "expected `startpos` or `fen <FEN>`, found {:?}",
                setup.join(" ")
            )));
        }
    };
    for &text in moves {
        let m = movegen::find_legal_move(&mut pos, text)
            .ok_or_else(|| PositionError::IllegalMove(text.to_string()))?;
        let made = pos.make_move(m);
        assert!(made, "find_legal_move returned the illegal move {m}");
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<&str> {
        text.split_whitespace().collect()
    }

    #[test]
    fn start_position_with_and_without_moves() {
        assert_eq!(
            parse_position(&[]).unwrap().to_fen(),
            Position::STARTPOS_FEN
        );
        assert_eq!(
            parse_position(&tokens("startpos")).unwrap().to_fen(),
            Position::STARTPOS_FEN
        );
        let pos = parse_position(&tokens("startpos moves e2e4 c7c5 g1f3")).unwrap();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2"
        );
    }

    #[test]
    fn fen_with_moves() {
        let pos = parse_position(&tokens(
            "fen r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1 moves e1g1 e8c8",
        ))
        .unwrap();
        assert_eq!(pos.to_fen(), "2kr3r/8/8/8/8/8/8/R4RK1 w - - 2 2");
    }

    #[test]
    fn special_moves_in_the_moves_list() {
        // An empty moves list is fine.
        assert_eq!(
            parse_position(&tokens("startpos moves")).unwrap(),
            Position::startpos()
        );
        // En passant: 1. e4 Nf6 2. e5 d5 3. exd6.
        let pos = parse_position(&tokens("startpos moves e2e4 g8f6 e4e5 d7d5 e5d6")).unwrap();
        assert_eq!(
            pos.to_fen(),
            "rnbqkb1r/ppp1pppp/3P1n2/8/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 3"
        );
        // Counters given in the FEN carry on from there.
        let pos =
            parse_position(&tokens("fen 4k3/8/8/8/8/8/8/R3K3 w Q - 7 30 moves a1a2")).unwrap();
        assert_eq!(pos.to_fen(), "4k3/8/8/8/8/8/R7/4K3 b - - 8 30");
    }

    #[test]
    fn reports_the_first_illegal_move() {
        // Castling is written as the king's move; king-takes-rook notation is not accepted.
        let error = parse_position(&tokens(
            "fen r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1 moves e1g1 e8h8 a2a3",
        ));
        assert_eq!(error, Err(PositionError::IllegalMove("e8h8".to_string())));
        let error = parse_position(&tokens("startpos moves e2e4 e7e5 e4e5 d7d5"));
        assert_eq!(error, Err(PositionError::IllegalMove("e4e5".to_string())));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            parse_position(&tokens("startpos moves e2e5")),
            Err(PositionError::IllegalMove(m)) if m == "e2e5"
        ));
        assert!(matches!(
            parse_position(&tokens("fen 8/8/8 w - - 0 1")),
            Err(PositionError::Fen(_))
        ));
        assert!(matches!(
            parse_position(&tokens("somewhere")),
            Err(PositionError::Syntax(_))
        ));
    }
}
