//! `bench`: a fixed-depth search of a fixed set of positions.
//!
//! The total node count fingerprints the search. A change that leaves it the same hasn't
//! changed what the engine searches, only how fast it does it. The positions and depth must
//! therefore never change by accident: doing so changes every later count.
//!
//! The search is deterministic: one thread, no time limits, and a fresh search for every
//! position, so the count is the same on every machine.

use crate::board::Position;
use crate::search::{self, Limits};
use crate::types::Move;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// The depth `backrank bench` uses when not given one.
pub const DEFAULT_DEPTH: u32 = 5;

/// The bench positions, as FEN.
///
/// The first six are the standard perft suite (see `tests/perft.rs`). The other 44 were
/// sampled from Backrank's own games against `sf-elo2000` on the Stockfish ladder (November
/// 2026): 12 openings and early middlegames (phase 20 to 24 of 24), 18 middlegames (9 to
/// 19) and 14 endgames (0 to 8). Each is at least eight plies into its game, not in check,
/// has a legal move, and has a halfmove clock of 40 or less.
pub const POSITIONS: [&str; 50] = [
    // The perft suite.
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    // Openings and early middlegames.
    "r2qk2r/ppp1bppp/2n2n2/5b2/4p3/1PP2NP1/P3PPBP/RNBQK2R w KQkq - 0 10",
    "r2q1rk1/pbp2pp1/1pn2b1p/3pp3/3P4/P3PNP1/1PPN1PBP/2RQ1RK1 w - - 0 13",
    "rnbq1rk1/3pppbp/1pp3p1/4P3/p4BP1/2NP1N2/PPP2P1P/R2QKB1R w KQ - 1 11",
    "rnb1kb1r/p1q1ppnp/2p3p1/1p2P3/8/2NB1N2/PPPB1PPP/R2QK2R w KQkq - 1 10",
    "1rbr2k1/1pp1qpp1/7p/4n3/1pPP4/7P/PBBQ1PP1/R3R1K1 b - - 2 20",
    "r3k1r1/pbq1bpp1/2n2n1p/pBp5/4p3/2P2N1Q/1P3PPP/RNB1R1K1 w q - 0 16",
    "rn2kb1r/ppq2ppp/2p1pn2/3p4/1P1P2b1/B1NBPN1P/P1P2PP1/R2Q1RK1 b kq - 0 9",
    "rn1qkb1r/pp3ppp/2pP1n2/8/2P5/7P/PP2PPBP/RNBQK2R w KQkq - 0 8",
    "rnb2rk1/pp4pp/1qpPpb1n/4Np2/3P1P2/2N5/PPP1B1PP/R1BQK2R b KQ - 4 10",
    "r4rk1/1pp1n1pp/2nq1p2/p4b2/3p3P/1P1P1NP1/PB2PPB1/2RQ1RK1 b - - 5 15",
    "r1q1kb1r/2p1pp2/1np5/P2pPp2/3P1P1p/5N2/P1PB2PP/R2Q1RK1 b kq - 0 16",
    "r1b1r1k1/1pp2p1n/p1npq3/4pp1p/2P1P2B/2PP2PP/P2Q2BN/R3K2R w KQ - 2 18",
    // Middlegames.
    "7k/3r2qB/pP1nb2R/3pP1Q1/2p5/2P5/1P6/2K2R2 w - - 5 45",
    "2r3k1/1p1r4/p5p1/4RpB1/3p3Q/3q2PP/5PK1/7B w - - 3 35",
    "r5k1/p1Rpppbp/6pn/1b1PP3/5P2/5NBP/PP4P1/3KR3 b - - 0 27",
    "2b4r/8/3p1Pkr/p1pP1n2/2P1NP2/P2KB3/8/3RR3 b - - 9 37",
    "6k1/ppp2rbp/4Q1p1/8/3Pp1nP/P5Pq/1PPB1P2/R4RK1 w - - 3 24",
    "2b3k1/p3n1pp/p1n2p2/2Q1q3/7N/3p2PP/1P1B1PB1/6K1 w - - 7 32",
    "6k1/1pp2pp1/2n4p/1pbp4/3p2qP/r7/1QPn1PB1/B3R1K1 w - - 0 27",
    "r1bk1b1r/1ppp1pp1/7p/p1nP4/5BP1/P2NK2B/1PP2P1P/R6R w - - 0 17",
    "3q2k1/5p2/p1ppb1p1/1r6/3PN3/4Q2P/PP2RPP1/6K1 w - - 7 25",
    "r5k1/pb1r2p1/1q3pQ1/1p1p1N2/8/5B1R/5R1P/6K1 w - - 3 40",
    "5R2/2p5/2r3k1/1r4p1/2p2p2/2B2P2/p4PK1/4R3 b - - 7 63",
    "r4b2/p2Nk3/1np4p/1p3pp1/5P2/1BP1P1PP/PP1N4/R1B1K2b b Q - 0 18",
    "4r1k1/5pb1/r5p1/8/5P2/3KP2P/PBP4P/1R5R b - - 2 29",
    "4r1r1/kp6/p6Q/8/N1P1p3/4P3/R3KP2/8 w - - 0 42",
    "4Q3/6qk/7r/p3p1b1/P1P5/3P4/2KB4/7R w - - 13 72",
    "3Q4/5k2/4p1pp/1qP1R3/8/6P1/PPP2P1P/R5K1 w - - 1 30",
    "3rr1k1/1pp3p1/1bn3p1/p3p3/b7/2qBP1P1/5P1P/4R1K1 b - - 3 31",
    "4k1r1/1p3p2/2p2p1p/2P4q/4Pb2/r5P1/P3bP1P/1R1Q1RK1 w - - 0 25",
    // Endgames.
    "6k1/1pp2pp1/2P4p/3p4/7P/b2p4/5Pq1/B3K3 b - - 0 32",
    "6k1/5r2/4Q3/5P2/p7/4K3/8/7R w - - 0 86",
    "8/6kp/5pp1/6P1/P3qP2/6K1/P7/R4R2 b - - 3 43",
    "5k2/5p2/2p5/2P5/4q2p/1K6/P1R5/8 w - - 0 51",
    "3k4/6Q1/8/4p2p/3bP3/4B2P/PPP2KP1/5R2 b - - 3 42",
    "8/6k1/1r1p3R/2pP2Pp/8/3K2P1/1P3r1P/1R6 w - - 7 42",
    "8/5pk1/4b1p1/p1r1N3/8/P6P/1P2RPP1/6K1 w - - 8 37",
    "r4rk1/1p3pnp/p7/8/4p3/b1P5/2P4P/3K1R2 b - - 1 28",
    "k6r/6p1/pQ2pn2/4p2p/1P2p3/2P1B1P1/1P3P1P/K7 w - - 0 38",
    "1r6/3k4/8/R3K3/4N3/8/8/8 b - - 8 74",
    "1r4k1/1pR2p2/1n3b1p/p2p3p/1p3P2/3P2PB/P2BKP2/8 w - - 4 38",
    "8/1k6/8/1P6/3BK3/P7/8/8 b - - 0 72",
    "5b1r/1p3p2/3k1n1p/3pp1p1/3B4/N3P2P/3P1PP1/5K1R w - - 0 21",
    "8/5k2/8/4p2p/4Np1K/2R2P2/r6P/8 w - - 16 58",
];

/// What searching one bench position found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionResult {
    /// The position's FEN, from [`POSITIONS`].
    pub fen: &'static str,
    /// The move the search chose.
    pub best: Move,
    /// Nodes searched, counting interior and quiescence nodes.
    pub nodes: u64,
    /// How long the search took.
    pub time: Duration,
}

/// The whole bench run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Total nodes over all positions: the bench's fingerprint.
    pub nodes: u64,
    /// Total search time, not counting setting up the positions.
    pub time: Duration,
}

impl Summary {
    /// Nodes per second, rounded down. Assumes at least a nanosecond passed.
    pub fn nps(&self) -> u64 {
        let nanos = self.time.as_nanos().max(1);
        (u128::from(self.nodes) * 1_000_000_000 / nanos) as u64
    }
}

/// Searches every bench position to `depth`, calling `each` after each one with its index
/// in [`POSITIONS`] and what it found.
///
/// # Panics
///
/// If `depth` is 0, or a bench position fails to parse or has no legal move. The unit tests
/// check that neither can happen with the built-in positions.
pub fn run(depth: u32, each: &mut dyn FnMut(usize, &PositionResult)) -> Summary {
    assert!(depth >= 1, "bench depth must be at least 1");
    let stop = AtomicBool::new(false);
    let mut nodes = 0;
    let mut time = Duration::ZERO;
    for (index, &fen) in POSITIONS.iter().enumerate() {
        let mut pos = Position::from_fen(fen).expect("bench positions are valid FEN");
        let limits = Limits {
            depth: Some(depth),
            ..Limits::default()
        };
        let mut searched = 0;
        let start = Instant::now();
        let best = search::search(&mut pos, &limits, &stop, &mut |info| searched = info.nodes);
        let elapsed = start.elapsed();
        let result = PositionResult {
            fen,
            best: best.expect("bench positions have a legal move"),
            nodes: searched,
            time: elapsed,
        };
        nodes += result.nodes;
        time += elapsed;
        each(index, &result);
    }
    Summary { nodes, time }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen;

    #[test]
    fn positions_are_valid_distinct_and_playable() {
        for (i, fen) in POSITIONS.iter().enumerate() {
            let mut pos = Position::from_fen(fen).unwrap_or_else(|e| panic!("{i}: {fen}: {e}"));
            assert_eq!(pos.to_fen(), *fen, "{i} doesn't round-trip");
            assert!(!movegen::legal_moves(&mut pos).is_empty(), "{i}: {fen}");
            assert!(!POSITIONS[..i].contains(fen), "{i} is a duplicate: {fen}");
        }
    }

    #[test]
    fn is_deterministic_and_adds_up() {
        let collect = || {
            let mut results = Vec::new();
            let summary = run(2, &mut |i, r| results.push((i, r.fen, r.best, r.nodes)));
            (summary.nodes, results)
        };
        let (total, results) = collect();
        assert_eq!(collect(), (total, results.clone()), "two runs differ");
        assert_eq!(results.len(), POSITIONS.len());
        assert!(
            results
                .iter()
                .enumerate()
                .all(|(i, r)| r.0 == i && r.1 == POSITIONS[i])
        );
        assert_eq!(total, results.iter().map(|r| r.3).sum::<u64>());
        assert!(results.iter().all(|r| r.3 > 0));
    }

    #[test]
    fn best_moves_are_legal() {
        run(1, &mut |_, r| {
            let mut pos = Position::from_fen(r.fen).unwrap();
            assert!(movegen::legal_moves(&mut pos).contains(&r.best), "{r:?}");
        });
    }

    #[test]
    fn nps_handles_short_and_long_runs() {
        let summary = |nodes, time| Summary { nodes, time };
        assert_eq!(summary(1_000, Duration::from_millis(500)).nps(), 2_000);
        assert_eq!(summary(5, Duration::ZERO).nps(), 5_000_000_000);
        // No overflow at u64::MAX nodes.
        assert_eq!(summary(u64::MAX, Duration::from_secs(1)).nps(), u64::MAX);
    }
}
