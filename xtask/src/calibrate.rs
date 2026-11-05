//! `cargo xtask calibrate`: measure ratings for the ladder rungs that don't have one.
//!
//! The blunder rungs sit below Stockfish's weakest rated setting, `sf-elo1320`. Working
//! down from there, each rung plays its stronger neighbour, and its rating is that
//! neighbour's rating plus the Elo difference measured. The ratings go to
//! `results/calibration.toml`, where `cargo xtask ladder` finds them.

use crate::fastchess::Match;
use crate::ladder::{self, Bound, Calibrated, Ladder};
use crate::stats::Elo;
use crate::{engine, paths, time};
use anyhow::{Context, Result, ensure};

#[derive(clap::Args)]
pub struct Args {
    /// Only calibrate this rung. Its stronger neighbour must already have a rating.
    #[arg(long)]
    rung: Option<String>,
    /// Games per rung (default: from tools/ladder.toml).
    #[arg(long)]
    games: Option<u32>,
    /// Time control as seconds+increment (default: from tools/ladder.toml).
    #[arg(long)]
    tc: Option<String>,
    /// Games played at the same time (default: from tools/ladder.toml).
    #[arg(long)]
    concurrency: Option<u32>,
}

pub fn run(args: &Args) -> Result<()> {
    ladder::ensure_tools()?;
    let ladder = Ladder::load()?;
    let mut calibration = ladder::load_calibration()?;
    let d = &ladder.defaults;
    let games = args.games.unwrap_or(d.games);
    ensure!(
        games >= 2 && games.is_multiple_of(2),
        "--games must be an even number of at least 2"
    );
    let tc = args.tc.clone().unwrap_or_else(|| d.tc.clone());
    let concurrency = args.concurrency.unwrap_or(d.concurrency);

    // The weakest rung with a rating of its own anchors the chain.
    let anchor = ladder
        .rungs
        .iter()
        .position(|r| r.uci_elo.is_some())
        .context("the ladder has no uci_elo rung to anchor calibration")?;
    let targets: Vec<usize> = match &args.rung {
        Some(name) => {
            let index = ladder.index(name)?;
            ensure!(
                index < anchor,
                "{name} has a rating of its own; only rungs below {} need calibrating",
                ladder.rungs[anchor].name
            );
            vec![index]
        }
        None => (0..anchor).rev().collect(),
    };

    let sfproxy = engine::build_sfproxy()?;
    let stockfish = paths::stockfish();
    let stockfish_name = engine::id_name(&stockfish)?;
    for index in targets {
        let rung = &ladder.rungs[index];
        let upper = &ladder.rungs[index + 1];
        let upper_rating = ladder::require_rating(&ladder, upper, &calibration)?;
        println!(
            "calibrate: {} ({}) vs {} (rated {upper_rating}), {games} games at {tc}",
            rung.name,
            rung.describe(),
            upper.name
        );
        let stamp = time::file_stamp();
        let pgn_path = paths::results_pgn().join(format!("calibrate-{}-{stamp}.pgn", rung.name));
        let log_path = paths::results_logs().join(format!("calibrate-{}-{stamp}.log", rung.name));
        let m = Match {
            engines: [
                rung.engine(&sfproxy, &stockfish),
                upper.engine(&sfproxy, &stockfish),
            ],
            tc: &tc,
            rounds: games / 2,
            concurrency,
            book: &paths::book(),
            pgn_out: &pgn_path,
            log: &log_path,
            seed: Some(ladder::random_seed()),
            sprt: None,
        };
        m.run()?;
        let tally = m.tally(&rung.name, &upper.name)?;
        ensure!(
            tally.abnormal.is_empty() && tally.unfinished == 0,
            "games ended abnormally ({:?}, {} unfinished); see {}",
            tally.abnormal,
            tally.unfinished,
            pgn_path.display()
        );
        let record = tally.record();

        // A whitewash only bounds the gap (by the rule of three), so the rating becomes a
        // bound. A bound only carries down the chain in its own direction: losing every
        // game to a rung rated "at least X" says nothing about an upper limit.
        let whitewash = match record.score() {
            s if s <= 0.0 => Some(Bound::AtMost),
            s if s >= 1.0 => Some(Bound::AtLeast),
            _ => None,
        };
        let bound = match (whitewash, upper_rating.bound) {
            (None, inherited) => Some(inherited),
            (Some(bound), Bound::Exact) => Some(bound),
            (Some(bound), inherited) if bound == inherited => Some(bound),
            _ => None,
        };
        let Some(bound) = bound else {
            println!(
                "calibrate: {} scored {:.0}% against {}, which only has a bound ({upper_rating}): \
                 no rating can be given. Move the blunder values closer and calibrate again.",
                rung.name,
                record.score() * 100.0,
                upper.name
            );
            calibration.remove(&rung.name);
            ladder::save_calibration(&calibration)?;
            break;
        };
        let rating = upper_rating.value + record.elo_or_bound();
        println!(
            "calibrate: {} scored {:.1}% (W{} D{} L{}), Elo {} against {}: rated {}{:.0}",
            rung.name,
            record.score() * 100.0,
            record.wins,
            record.draws,
            record.losses,
            Elo(record.elo()),
            upper.name,
            match bound {
                Bound::Exact => "",
                Bound::AtMost => "at most ",
                Bound::AtLeast => "at least ",
            },
            rating
        );
        if bound != Bound::Exact && upper_rating.bound == Bound::Exact {
            println!(
                "calibrate: warning: {} and {} are too far apart to measure; consider moving \
                 their blunder values closer",
                rung.name, upper.name
            );
        }
        calibration.insert(
            rung.name.clone(),
            Calibrated {
                rating,
                bound,
                blunder: rung.blunder.unwrap_or(1.0),
                against: upper.name.clone(),
                against_rating: upper_rating.value,
                games: record.games(),
                score: record.score(),
                tc: tc.clone(),
                stockfish: stockfish_name.clone(),
                date: time::date_time(),
            },
        );
        // Save after each rung, so an interrupted run keeps what it measured.
        ladder::save_calibration(&calibration)?;
    }
    println!(
        "calibrate: ratings saved to {}",
        paths::calibration().display()
    );
    Ok(())
}
