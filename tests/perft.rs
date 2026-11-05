//! Perft suite from docs/plan-v1.md §7.2 (chessprogramming.org "Perft Results", deepest
//! counts checked against Stockfish 19).
//!
//! The fast depths run with every `cargo test`. The deep depths take tens of seconds even in
//! release mode, so they are ignored by default; run them with
//! `cargo test --release --test perft -- --ignored`.

use backrank::Position;
use backrank::perft::perft;

/// Checks `perft(depth) == counts[depth - 1]` for each depth in `depths`.
fn check(fen: &str, counts: &[u64], depths: std::ops::RangeInclusive<u32>) {
    let mut pos = Position::from_fen(fen).unwrap();
    for depth in depths {
        assert_eq!(
            perft(&mut pos, depth),
            counts[depth as usize - 1],
            "perft({depth}) of {fen}"
        );
    }
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const START_COUNTS: &[u64] = &[20, 400, 8_902, 197_281, 4_865_609, 119_060_324];

const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const KIWIPETE_COUNTS: &[u64] = &[48, 2_039, 97_862, 4_085_603, 193_690_690];

const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POS3_COUNTS: &[u64] = &[14, 191, 2_812, 43_238, 674_624, 11_030_083];

const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POS4_MIRRORED: &str = "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1";
const POS4_COUNTS: &[u64] = &[6, 264, 9_467, 422_333, 15_833_292];

const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POS5_COUNTS: &[u64] = &[44, 1_486, 62_379, 2_103_487, 89_941_194];

const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";
const POS6_COUNTS: &[u64] = &[46, 2_079, 89_890, 3_894_594, 164_075_551];

#[test]
fn start_fast() {
    check(START, START_COUNTS, 1..=4);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn start_deep() {
    check(START, START_COUNTS, 5..=6);
}

#[test]
fn kiwipete_fast() {
    check(KIWIPETE, KIWIPETE_COUNTS, 1..=3);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn kiwipete_deep() {
    check(KIWIPETE, KIWIPETE_COUNTS, 4..=5);
}

#[test]
fn pos3_fast() {
    check(POS3, POS3_COUNTS, 1..=5);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn pos3_deep() {
    check(POS3, POS3_COUNTS, 6..=6);
}

#[test]
fn pos4_fast() {
    check(POS4, POS4_COUNTS, 1..=4);
    check(POS4_MIRRORED, POS4_COUNTS, 1..=4);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn pos4_deep() {
    check(POS4, POS4_COUNTS, 5..=5);
    check(POS4_MIRRORED, POS4_COUNTS, 5..=5);
}

#[test]
fn pos5_fast() {
    check(POS5, POS5_COUNTS, 1..=3);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn pos5_deep() {
    check(POS5, POS5_COUNTS, 4..=5);
}

#[test]
fn pos6_fast() {
    check(POS6, POS6_COUNTS, 1..=3);
}

#[test]
#[ignore = "deep perft: run with `cargo test --release --test perft -- --ignored`"]
fn pos6_deep() {
    check(POS6, POS6_COUNTS, 4..=5);
}
