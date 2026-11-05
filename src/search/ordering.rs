//! Move ordering. Alpha-beta cuts off sooner when the best moves are tried first.
//!
//! The order is: the move from the previous iteration's principal variation, then
//! captures and queen promotions by MVV-LVA (most valuable victim first, then least
//! valuable attacker), then quiet moves.

use crate::board::Position;
use crate::movegen::{MAX_MOVES, MoveList};
use crate::types::{Move, PieceType};

const PV_MOVE: i32 = 1 << 20;
const TACTICAL: i32 = 1 << 16;

/// Ordering rank of each piece type for MVV-LVA, pawn lowest, indexed by
/// [`PieceType::index`]. The king ranks highest as an attacker, so it captures last.
const RANK: [i32; 6] = [1, 2, 3, 4, 5, 6];

/// Hands out a list's moves best first. Each step finds the best remaining move (selection
/// sort), so when a cutoff comes after a few moves, the rest are never sorted.
pub(super) struct Ordered {
    moves: MoveList,
    scores: [i32; MAX_MOVES],
    next: usize,
}

impl Ordered {
    pub(super) fn new(pos: &Position, moves: MoveList, pv_move: Option<Move>) -> Ordered {
        let mut scores = [0; MAX_MOVES];
        for (score, &m) in scores.iter_mut().zip(moves.iter()) {
            *score = order_score(pos, m, pv_move);
        }
        Ordered {
            moves,
            scores,
            next: 0,
        }
    }
}

impl Iterator for Ordered {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        let len = self.moves.len();
        if self.next >= len {
            return None;
        }
        let mut best = self.next;
        for i in self.next + 1..len {
            if self.scores[i] > self.scores[best] {
                best = i;
            }
        }
        self.moves.swap(self.next, best);
        self.scores.swap(self.next, best);
        self.next += 1;
        Some(self.moves[self.next - 1])
    }
}

fn order_score(pos: &Position, m: Move, pv_move: Option<Move>) -> i32 {
    if Some(m) == pv_move {
        return PV_MOVE;
    }
    let mut score = 0;
    if m.is_capture() {
        let victim = if m.is_en_passant() {
            PieceType::Pawn
        } else {
            pos.piece_at(m.to()).expect("a capture has a victim").kind()
        };
        let attacker = pos.piece_at(m.from()).expect("a move has a piece").kind();
        score += TACTICAL + 10 * RANK[victim.index()] - RANK[attacker.index()];
    }
    if m.promotion_piece() == Some(PieceType::Queen) {
        score += TACTICAL + 10 * RANK[PieceType::Queen.index()];
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen;

    fn ordered(fen: &str, pv: Option<&str>) -> Vec<String> {
        let mut pos = Position::from_fen(fen).unwrap();
        let pv_move = pv.map(|text| movegen::find_legal_move(&mut pos, text).unwrap());
        let mut moves = MoveList::new();
        movegen::generate(&pos, &mut moves);
        Ordered::new(&pos, moves, pv_move)
            .map(|m| m.to_string())
            .collect()
    }

    #[test]
    fn most_valuable_victim_then_least_valuable_attacker() {
        // White can take the d5 queen with the pawn or the knight, or the b6 pawn with
        // the bishop.
        let moves = ordered("4k3/8/1p6/3q4/3BP3/2N5/8/4K3 w - - 0 1", None);
        assert_eq!(&moves[..3], ["e4d5", "c3d5", "d4b6"]);
        // Then the quiet moves.
        assert!(
            moves[3..]
                .iter()
                .all(|m| !["e4d5", "c3d5", "d4b6"].contains(&m.as_str()))
        );
    }

    #[test]
    fn principal_variation_move_comes_first() {
        let moves = ordered("4k3/8/8/3q4/4P3/2N5/8/4K3 w - - 0 1", Some("e1f2"));
        assert_eq!(moves[0], "e1f2");
        assert_eq!(moves[1], "e4d5");
    }

    #[test]
    fn queen_promotions_are_tried_early() {
        let moves = ordered("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1", None);
        assert_eq!(moves[0], "b7b8q");
    }

    #[test]
    fn hands_out_every_move_once() {
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        let mut moves = ordered(fen, None);
        let mut pos = Position::from_fen(fen).unwrap();
        let mut generated = MoveList::new();
        movegen::generate(&pos, &mut generated);
        let mut expected: Vec<String> = generated.iter().map(|m| m.to_string()).collect();
        moves.sort();
        expected.sort();
        assert_eq!(moves, expected);
        assert_eq!(movegen::legal_moves(&mut pos).len(), 48);
    }
}
