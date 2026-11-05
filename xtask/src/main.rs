//! Development tooling for Backrank, run as `cargo xtask <command>`.

mod calibrate;
mod engine;
mod fastchess;
mod ladder;
mod paths;
mod perft_diff;
mod pgn;
mod selfplay;
mod setup;
mod smoke;
mod sprt;
mod stats;
mod time;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "xtask",
    bin_name = "cargo xtask",
    about = "Backrank development tasks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download fastchess and the opening book, and snapshot Stockfish into tools/.
    Setup(setup::Args),
    /// Check the match harness: Stockfish at Skill Level 0 vs full-strength Stockfish.
    Smoke(smoke::Args),
    /// Compare Backrank's perft with Stockfish's and narrow any difference down to the
    /// exact position and move.
    PerftDiff(perft_diff::Args),
    /// Play Backrank against itself and fail on any crash, illegal move or time forfeit.
    Selfplay(selfplay::Args),
    /// Play Backrank against its rung of the Stockfish ladder and record the result.
    Ladder(ladder::Args),
    /// Measure ratings for the ladder rungs below Stockfish's weakest rated setting.
    Calibrate(calibrate::Args),
    /// Test whether the working tree beats an earlier commit (sequential probability
    /// ratio test).
    Sprt(sprt::Args),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Setup(args) => setup::run(&args),
        Command::Smoke(args) => smoke::run(&args),
        Command::PerftDiff(args) => perft_diff::run(&args),
        Command::Selfplay(args) => selfplay::run(&args),
        Command::Ladder(args) => ladder::run(&args),
        Command::Calibrate(args) => calibrate::run(&args),
        Command::Sprt(args) => sprt::run(&args),
    }
}
