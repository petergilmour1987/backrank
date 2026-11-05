//! The `backrank` binary: argument handling and output only; the work happens in the library.

use backrank::{perft, uci};
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "\
usage: backrank                     (prints the engine name; the UCI loop comes later)
       backrank perft <depth> [startpos | fen <FEN>] [moves <move>...]";

fn main() -> ExitCode {
    backrank::attacks::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            println!("{} {}", backrank::ENGINE_NAME, backrank::ENGINE_VERSION);
            ExitCode::SUCCESS
        }
        Some("perft") => perft_command(&args[1..]),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => usage_error(&format!("unknown command {other:?}")),
    }
}

/// Prints each legal move with its leaf count, then the total, in the same format as
/// Stockfish's `go perft`. Timing goes to stderr so stdout can be compared directly.
fn perft_command(args: &[String]) -> ExitCode {
    let Some(depth) = args
        .first()
        .and_then(|d| d.parse::<u32>().ok())
        .filter(|&d| d >= 1)
    else {
        return usage_error("perft needs a depth of at least 1");
    };
    // Accept the FEN either as separate arguments or as one quoted argument.
    let tokens: Vec<&str> = args[1..]
        .iter()
        .flat_map(|a| a.split_whitespace())
        .collect();
    let mut pos = match uci::parse_position(&tokens) {
        Ok(pos) => pos,
        Err(e) => return usage_error(&e.to_string()),
    };

    let start = Instant::now();
    let split = perft::divide(&mut pos, depth);
    let elapsed = start.elapsed();
    let total: u64 = split.iter().map(|&(_, nodes)| nodes).sum();
    for (m, nodes) in &split {
        println!("{m}: {nodes}");
    }
    println!();
    println!("Nodes searched: {total}");
    let nps = total as f64 / elapsed.as_secs_f64().max(1e-9);
    eprintln!(
        "{:.3} s, {:.1} million nodes/s",
        elapsed.as_secs_f64(),
        nps / 1e6
    );
    ExitCode::SUCCESS
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("error: {message}\n{USAGE}");
    ExitCode::from(2)
}
