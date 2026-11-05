//! `cargo xtask sprt`: is the current code better than an earlier version?
//!
//! The working tree plays a git ref through fastchess's sequential probability ratio test,
//! which stops as soon as the evidence is strong enough either way. H1 accepted means the
//! change gains at least `elo1`; H0 accepted means it gains no more than `elo0`.

use crate::fastchess::{EngineSpec, Match, Sprt};
use crate::ladder::{self, append_csv};
use crate::stats::Elo;
use crate::{engine, paths, time};
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;
use std::process::Command;

#[derive(clap::Args)]
pub struct Args {
    /// The version to compare against: any git ref, such as a commit, tag or `HEAD~1`.
    #[arg(long, default_value = "HEAD")]
    base: String,
    /// Elo gain under H0, the "no real gain" hypothesis (normalized Elo).
    #[arg(long, default_value_t = 0.0)]
    elo0: f64,
    /// Elo gain under H1, the "worth having" hypothesis (normalized Elo).
    #[arg(long, default_value_t = 10.0)]
    elo1: f64,
    /// Chance of accepting H1 when H0 is true.
    #[arg(long, default_value_t = 0.05)]
    alpha: f64,
    /// Chance of accepting H0 when H1 is true.
    #[arg(long, default_value_t = 0.05)]
    beta: f64,
    /// Time control as seconds+increment.
    #[arg(long, default_value = "8+0.08")]
    tc: String,
    /// Games played at the same time.
    #[arg(long, default_value_t = 8)]
    concurrency: u32,
    /// Give up without a verdict after this many games.
    #[arg(long, default_value_t = 20_000)]
    max_games: u32,
}

pub fn run(args: &Args) -> Result<()> {
    ensure!(args.elo0 < args.elo1, "--elo0 must be below --elo1");
    ensure!(args.max_games >= 2, "--max-games must be at least 2");
    for required in [paths::fastchess(), paths::book()] {
        ensure!(
            required.exists(),
            "{} is missing; run `cargo xtask setup` first",
            required.display()
        );
    }

    let new_label = engine::version_label()?;
    let new = engine::snapshot(&engine::build_backrank()?, &new_label)?;
    let base_label = engine::git(&["rev-parse", "--short", &format!("{}^{{commit}}", args.base)])
        .with_context(|| format!("{} is not a commit", args.base))?;
    let base = base_binary(&base_label)?;
    // Fail fast on a base from before the UCI loop existed, or a broken cached build.
    for (name, binary) in [("new", &new), ("base", &base)] {
        engine::id_name(binary).with_context(|| {
            format!(
                "the {name} engine ({}) doesn't answer UCI",
                binary.display()
            )
        })?;
    }

    let new_name = format!("new-{new_label}");
    let base_name = format!("base-{base_label}");
    println!(
        "sprt: {new_name} vs {base_name}, [{}, {}] at {}",
        args.elo0, args.elo1, args.tc
    );
    let stamp = time::file_stamp();
    let pgn_path = paths::results_pgn().join(format!("sprt-{stamp}.pgn"));
    let log_path = paths::results_logs().join(format!("sprt-{stamp}.log"));
    let seed = ladder::random_seed();
    let m = Match {
        engines: [
            EngineSpec::new(&new_name, &new),
            EngineSpec::new(&base_name, &base),
        ],
        tc: &args.tc,
        rounds: args.max_games.div_ceil(2),
        concurrency: args.concurrency,
        book: &paths::book(),
        pgn_out: &pgn_path,
        log: &log_path,
        seed: Some(seed),
        sprt: Some(Sprt {
            elo0: args.elo0,
            elo1: args.elo1,
            alpha: args.alpha,
            beta: args.beta,
        }),
    };
    let output = m.run_capturing()?;
    let tally = m.tally(&new_name, &base_name)?;
    let record = tally.record();
    let verdict = if output.contains("H1 was accepted") {
        "H1 accepted: the new version is better"
    } else if output.contains("H0 was accepted") {
        "H0 accepted: no gain"
    } else {
        "inconclusive: no verdict within --max-games"
    };
    let (low, high) = record.elo_interval();
    println!();
    println!(
        "sprt: {verdict}. W{} D{} L{} ({:.1}%), Elo {} [{}, {}]",
        record.wins,
        record.draws,
        record.losses,
        record.score() * 100.0,
        Elo(record.elo()),
        Elo(low),
        Elo(high)
    );
    if !tally.own_faults.is_empty() {
        println!(
            "sprt: BUG: the new version lost games by {:?}",
            tally.own_faults
        );
    }
    if !tally.opponent_faults.is_empty() {
        println!(
            "sprt: warning: the base version lost games by {:?}, so the result overstates \
             the new version",
            tally.opponent_faults
        );
    }

    let number = |x: f64| {
        if x.is_finite() {
            format!("{x:.1}")
        } else {
            String::new()
        }
    };
    append_csv(
        &paths::sprt_csv(),
        "date,new,base,tc,elo0,elo1,alpha,beta,games,wins,draws,losses,elo,elo_low,elo_high,\
         new_faults,base_faults,seed,result",
        &[
            time::date_time(),
            new_label,
            base_label,
            args.tc.clone(),
            args.elo0.to_string(),
            args.elo1.to_string(),
            args.alpha.to_string(),
            args.beta.to_string(),
            record.games().to_string(),
            record.wins.to_string(),
            record.draws.to_string(),
            record.losses.to_string(),
            number(record.elo()),
            number(low),
            number(high),
            tally.own_faults.values().sum::<u32>().to_string(),
            tally.opponent_faults.values().sum::<u32>().to_string(),
            seed.to_string(),
            verdict.split(':').next().unwrap_or(verdict).to_string(),
        ],
    )?;
    println!("sprt: result added to {}", paths::sprt_csv().display());
    Ok(())
}

/// The release binary for commit `sha`, built in a temporary git worktree. Builds of a
/// commit never change, so a snapshot from an earlier run is reused.
fn base_binary(sha: &str) -> Result<PathBuf> {
    let snapshot = paths::engines().join(format!("backrank-{sha}"));
    if snapshot.exists() {
        return Ok(snapshot);
    }
    let worktree = paths::root().join("target/sprt-worktree");
    if worktree.exists() {
        let _ = engine::git(&[
            "worktree",
            "remove",
            "--force",
            &worktree.display().to_string(),
        ]);
        let _ = std::fs::remove_dir_all(&worktree);
    }
    engine::git(&["worktree", "prune"])?;
    engine::git(&[
        "worktree",
        "add",
        "--detach",
        &worktree.display().to_string(),
        sha,
    ])?;
    // A target directory of its own keeps its build cache between runs without disturbing
    // the main one.
    let target = paths::root().join("target/sprt-build");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let built = Command::new(cargo)
        .args([
            "build",
            "--release",
            "--quiet",
            "--package",
            "backrank",
            "--bin",
            "backrank",
        ])
        .arg("--target-dir")
        .arg(&target)
        .current_dir(&worktree)
        .status();
    // Remove the worktree whatever happened to the build.
    let removed = engine::git(&[
        "worktree",
        "remove",
        "--force",
        &worktree.display().to_string(),
    ]);
    let status = built.context("failed to run cargo build")?;
    ensure!(status.success(), "building {sha} failed");
    if let Err(e) = removed {
        println!("sprt: warning: couldn't remove the temporary worktree: {e:#}");
    }
    engine::snapshot(&target.join("release/backrank"), sha)
}
