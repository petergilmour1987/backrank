//! Development tooling for Backrank, run as `cargo xtask <command>`.

mod engine;
mod fastchess;
mod paths;
mod pgn;
mod setup;
mod smoke;
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Setup(args) => setup::run(&args),
        Command::Smoke(args) => smoke::run(&args),
    }
}
