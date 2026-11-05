//! `cargo xtask smoke`: prove the match plumbing works before our engine exists.
//!
//! Stockfish at `Skill Level 0` plays full-strength Stockfish. Full strength should win
//! almost every game. If it only scores about 50%, the `Skill Level` option did not reach
//! the weak engine; if games end abnormally, the harness itself has a problem.

use crate::fastchess::{EngineSpec, Match};
use crate::{engine, paths, pgn, time};
use anyhow::{Context, Result, bail, ensure};

/// Full strength must score at least this much against Skill Level 0.
const MIN_STRONG_SCORE: f64 = 0.9;

#[derive(clap::Args)]
pub struct Args {
    /// Number of games (even: each opening is played from both sides).
    #[arg(long, default_value_t = 20)]
    games: u32,
    /// Time control in Cute Chess format (seconds+increment).
    #[arg(long, default_value = "2+0.02")]
    tc: String,
    /// Games played at the same time.
    #[arg(long, default_value_t = 8)]
    concurrency: u32,
}

pub fn run(args: &Args) -> Result<()> {
    ensure!(
        args.games >= 2 && args.games.is_multiple_of(2),
        "--games must be an even number of at least 2"
    );
    let stockfish = paths::stockfish();
    for required in [paths::fastchess(), paths::book(), stockfish.clone()] {
        ensure!(
            required.exists(),
            "{} is missing; run `cargo xtask setup` first",
            required.display()
        );
    }
    println!(
        "smoke: {} (Skill Level 0) vs itself at full strength",
        engine::id_name(&stockfish)?
    );

    let stamp = time::file_stamp();
    let pgn_path = paths::results_pgn().join(format!("smoke-{stamp}.pgn"));
    let log_path = paths::results_logs().join(format!("smoke-{stamp}.log"));
    Match {
        engines: [
            EngineSpec::new("sf-skill0", &stockfish).option("Skill Level", "0"),
            EngineSpec::new("sf-full", &stockfish),
        ],
        tc: &args.tc,
        rounds: args.games / 2,
        concurrency: args.concurrency,
        book: &paths::book(),
        pgn_out: &pgn_path,
        log: &log_path,
    }
    .run()?;

    let text = std::fs::read_to_string(&pgn_path)
        .with_context(|| format!("fastchess wrote no PGN at {}", pgn_path.display()))?;
    let games = pgn::parse(&text)?;
    let strong = pgn::Tally::for_engine(&games, "sf-full");
    println!(
        "smoke: sf-full W{} D{} L{} (score {:.1}%), unfinished {}, abnormal {:?}",
        strong.wins,
        strong.draws,
        strong.losses,
        strong.score() * 100.0,
        strong.unfinished,
        strong.abnormal
    );
    println!("smoke: games saved to {}", pgn_path.display());

    if strong.games() != args.games {
        bail!(
            "expected {} finished games, PGN has {}",
            args.games,
            strong.games()
        );
    }
    if !strong.abnormal.is_empty() {
        bail!("games ended abnormally: {:?}", strong.abnormal);
    }
    if strong.score() < MIN_STRONG_SCORE {
        bail!(
            "full-strength Stockfish scored only {:.1}% (need {:.0}%): is `Skill Level` reaching the engine?",
            strong.score() * 100.0,
            MIN_STRONG_SCORE * 100.0
        );
    }
    println!("smoke: PASS");
    Ok(())
}
