//! `cargo xtask selfplay`: Backrank against itself through fastchess.
//!
//! The result itself means nothing; the point is that every game must end normally. Any
//! crash, illegal move, time forfeit or stalled game is a bug in the engine.

use crate::fastchess::{EngineSpec, Match};
use crate::{engine, paths, pgn, time};
use anyhow::{Context, Result, bail, ensure};

#[derive(clap::Args)]
pub struct Args {
    /// Number of games (even: each opening is played from both sides).
    #[arg(long, default_value_t = 200)]
    games: u32,
    /// Time control in Cute Chess format (seconds+increment).
    #[arg(long, default_value = "4+0.04")]
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
    for required in [paths::fastchess(), paths::book()] {
        ensure!(
            required.exists(),
            "{} is missing; run `cargo xtask setup` first",
            required.display()
        );
    }
    let binary = engine::build_backrank()?;
    println!("selfplay: {} against itself", engine::id_name(&binary)?);

    let stamp = time::file_stamp();
    let pgn_path = paths::results_pgn().join(format!("selfplay-{stamp}.pgn"));
    let log_path = paths::results_logs().join(format!("selfplay-{stamp}.log"));
    Match {
        engines: [
            EngineSpec::new("backrank-a", &binary),
            EngineSpec::new("backrank-b", &binary),
        ],
        tc: &args.tc,
        rounds: args.games / 2,
        concurrency: args.concurrency,
        book: &paths::book(),
        pgn_out: &pgn_path,
        log: &log_path,
        seed: None,
        sprt: None,
    }
    .run()?;

    let text = std::fs::read_to_string(&pgn_path)
        .with_context(|| format!("fastchess wrote no PGN at {}", pgn_path.display()))?;
    let games = pgn::parse(&text)?;
    let tally = pgn::Tally::for_engine(&games, "backrank-a");
    println!(
        "selfplay: backrank-a W{} D{} L{}, unfinished {}, abnormal {:?}",
        tally.wins, tally.draws, tally.losses, tally.unfinished, tally.abnormal
    );
    println!("selfplay: games saved to {}", pgn_path.display());
    if tally.games() != args.games {
        bail!(
            "expected {} finished games, PGN has {}",
            args.games,
            tally.games()
        );
    }
    if !tally.abnormal.is_empty() {
        bail!("games ended abnormally: {:?}", tally.abnormal);
    }
    // fastchess logs at warning level and above, so any entry means something went wrong
    // (for example an engine reporting a PV that doesn't start with its move).
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let warnings: Vec<&str> = log.lines().filter(|l| l.starts_with('[')).collect();
    if !warnings.is_empty() {
        bail!(
            "fastchess logged {} warnings, starting with:\n{}\n(full log: {})",
            warnings.len(),
            warnings[..warnings.len().min(3)].join("\n"),
            log_path.display()
        );
    }
    println!("selfplay: PASS");
    Ok(())
}
