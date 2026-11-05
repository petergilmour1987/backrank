//! Search tests through the public API. Each mate was confirmed with Stockfish 19. These
//! check correctness, not strength: playing strength is measured by `cargo xtask ladder`.

use backrank::search::{Limits, Score, search};
use backrank::{Move, Position};
use std::sync::atomic::AtomicBool;

fn solve(fen: &str, depth: u32) -> (Move, Score) {
    let mut pos = Position::from_fen(fen).unwrap();
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    let mut score = None;
    let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |info| {
        score = Some(info.score);
    });
    (best.expect("the position has legal moves"), score.unwrap())
}

#[test]
fn mate_in_one() {
    for (fen, mv) in [
        ("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", "a1a8"),
        (
            "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
            "f3f7",
        ),
        ("7k/8/5K2/6Q1/8/8/8/8 w - - 0 1", "g5g7"),
    ] {
        let (best, score) = solve(fen, 2);
        assert_eq!(best.to_string(), mv, "{fen}");
        assert_eq!(score, Score::Mate(1), "{fen}");
    }
}

#[test]
fn mate_in_two() {
    // A quiet king move, then mate.
    let (_, score) = solve("7k/8/8/5K2/6Q1/8/8/8 w - - 0 1", 4);
    assert_eq!(score, Score::Mate(2));
    // A queen sacrifice: Qh6+ Kxh6 Bxf6#.
    let (best, score) = solve(
        "r1bq2r1/b4pk1/p1pp1p2/1p2pP2/1P2P1PB/3P4/1PPQ2P1/R3K2R w - - 0 1",
        4,
    );
    assert_eq!(best.to_string(), "d2h6");
    assert_eq!(score, Score::Mate(2));
    // For black: Rg1+ Kxg1 Rxf1#.
    let (best, score) = solve("6k1/pp4p1/2p5/2bp4/8/P5Pb/1P3rrP/2BRRN1K b - - 0 1", 4);
    assert_eq!(best.to_string(), "g2g1");
    assert_eq!(score, Score::Mate(2));
}

#[test]
fn the_check_extension_sees_mate_at_the_horizon() {
    // Depth 1 reaches the mated position with no depth left; only the extension makes it
    // a full node, where the lack of legal moves shows.
    let (best, score) = solve("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 1);
    assert_eq!(best.to_string(), "a1a8");
    assert_eq!(score, Score::Mate(1));
    // Likewise mate in two at depth 3, one ply less than it takes without.
    let (_, score) = solve("7k/8/8/5K2/6Q1/8/8/8 w - - 0 1", 3);
    assert_eq!(score, Score::Mate(2));
}

#[test]
fn being_mated_is_reported_with_a_negative_count() {
    // Whatever black plays, Qg7 or Qh7 style mates follow: Kh7 walks into Qg7#.
    let (_, score) = solve("7k/8/5K2/8/6Q1/8/8/8 b - - 0 1", 4);
    assert_eq!(score, Score::Mate(-1));
}

#[test]
fn stalemate_scores_zero_so_the_winner_avoids_it() {
    // Qg6 would stalemate black at once, so white must not play it.
    let (best, _) = solve("7k/8/5K2/8/6Q1/8/8/8 w - - 0 1", 3);
    assert_ne!(best.to_string(), "g4g6");
}
