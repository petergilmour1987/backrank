//! Search tests through the public API. Each mate was confirmed with Stockfish 19. These
//! check correctness, not strength: playing strength is measured by `cargo xtask ladder`.

use backrank::search::{Info, Limits, Score, search};
use backrank::{Move, Position, movegen};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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
fn quiescence_sees_mate_by_capture() {
    // Whatever black plays, Qxg7# follows, with the bishop behind the queen. At depth 1 that
    // capture is only seen in quiescence, which must notice the mate rather than stand pat
    // on winning a pawn.
    let (_, score) = solve("7k/6p1/6Pp/8/3Q4/8/1B6/6K1 b - - 0 1", 1);
    assert_eq!(score, Score::Mate(-1));
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

const MIDDLEGAMES: [&str; 4] = [
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
];

/// Plays `pv` from `fen`, panicking if any move isn't legal where it's played.
fn assert_pv_is_legal(fen: &str, info: &Info) {
    let mut pos = Position::from_fen(fen).unwrap();
    assert!(!info.pv.is_empty(), "empty PV in {info:?}");
    for m in &info.pv {
        let legal = movegen::find_legal_move(&mut pos, &m.to_string());
        let legal = legal.unwrap_or_else(|| panic!("{m} isn't legal in {info:?} from {fen}"));
        assert!(pos.make_move(legal));
    }
}

#[test]
fn every_reported_line_is_legal_and_starts_with_the_move_played() {
    for fen in MIDDLEGAMES {
        let mut pos = Position::from_fen(fen).unwrap();
        let limits = Limits {
            depth: Some(4),
            ..Limits::default()
        };
        let mut infos = Vec::new();
        let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |i| {
            infos.push(i.clone())
        });
        assert_eq!(infos.len(), 4, "{fen}");
        for info in &infos {
            assert_pv_is_legal(fen, info);
        }
        assert_eq!(infos.last().unwrap().pv.first().copied(), best, "{fen}");
    }
}

#[test]
fn a_clock_search_stays_within_its_budget() {
    // 2000 ms left: (2000 - 20) / 20 = 99 ms at most. The margin allows for a loaded machine
    // running debug builds.
    let mut pos = Position::from_fen(MIDDLEGAMES[0]).unwrap();
    let limits = Limits {
        time_left: Some(Duration::from_millis(2000)),
        move_overhead: Duration::from_millis(20),
        ..Limits::default()
    };
    let started = Instant::now();
    let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |_| {});
    assert!(best.is_some());
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_clock_with_no_time_left_still_gives_a_move() {
    let mut pos = Position::from_fen(MIDDLEGAMES[1]).unwrap();
    let limits = Limits {
        time_left: Some(Duration::ZERO),
        move_overhead: Duration::from_millis(20),
        ..Limits::default()
    };
    let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |_| {});
    assert!(movegen::legal_moves(&mut pos).contains(&best.unwrap()));
}

#[test]
fn stopping_mid_search_plays_the_last_completed_iteration() {
    let fen = MIDDLEGAMES[0];
    let mut pos = Position::from_fen(fen).unwrap();
    let limits = Limits {
        infinite: true,
        ..Limits::default()
    };
    let stop = AtomicBool::new(false);
    let mut infos = Vec::new();
    let best = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            stop.store(true, Ordering::Relaxed);
        });
        search(&mut pos, &limits, &stop, &mut |i| infos.push(i.clone()))
    });
    let last = infos.last().expect("depth 1 always completes");
    assert_eq!(last.pv.first().copied(), best);
    assert_pv_is_legal(fen, last);
    assert_eq!(
        pos,
        Position::from_fen(fen).unwrap(),
        "a stopped search restores the position"
    );
}

#[test]
fn check_heavy_positions_search_safely() {
    // Queens on an open board give long chains of checks, which the check extension
    // follows. This must finish and leave the position intact.
    let fen = "4k3/8/8/8/8/8/8/Q3K2q w - - 0 1";
    let mut pos = Position::from_fen(fen).unwrap();
    let limits = Limits {
        depth: Some(64),
        nodes: Some(200_000),
        ..Limits::default()
    };
    let mut last = None;
    let best = search(&mut pos, &limits, &AtomicBool::new(false), &mut |i| {
        last = Some(i.clone())
    });
    assert!(movegen::legal_moves(&mut pos).contains(&best.unwrap()));
    assert_pv_is_legal(fen, &last.unwrap());
    assert_eq!(pos, Position::from_fen(fen).unwrap());
}
