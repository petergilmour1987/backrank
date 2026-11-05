//! `sfproxy`: Stockfish at Skill Level 0 that sometimes plays a random move on purpose.
//!
//! It passes UCI through to Stockfish unchanged, except for `go`. Before each `go`, with
//! probability `--blunder`, it answers itself with a uniformly random legal move instead of
//! asking Stockfish. That gives the ladder opponents between a pure random mover
//! (`--blunder 1`) and Stockfish's weakest setting (`--blunder 0`). Legal moves come from
//! Backrank's own move generator, which perft has verified. A random move is reported with
//! Stockfish's depth-1 score for the position, so match runners see an honest score.

use anyhow::{Context, Result, ensure};
use backrank::{Position, movegen, uci};
use clap::Parser;
use std::collections::VecDeque;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(about = "Stockfish at Skill Level 0 that sometimes plays a random move")]
struct Args {
    /// The Stockfish binary to run.
    #[arg(long)]
    stockfish: PathBuf,
    /// Probability of playing a random legal move instead of Stockfish's, from 0 to 1.
    #[arg(long)]
    blunder: f64,
    /// Seed for the random choices (default: different every run).
    #[arg(long)]
    seed: Option<u64>,
}

/// SplitMix64: small, fast and good enough for picking moves.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// True with probability `p`.
    fn chance(&mut self, p: f64) -> bool {
        // The top 53 bits as a uniform value in [0, 1).
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) < p
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// A request to Stockfish whose reply needs routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    /// The GUI's `isready`: its `readyok` goes to the GUI.
    GuiIsReady,
    /// The proxy's own `go depth 1`: everything up to `bestmove` comes back to the proxy.
    OwnSearch,
}

/// The `score ...` of an info line for the principal variation (MultiPV line 1, or a line
/// without MultiPV), such as `cp 31` or `mate -2`.
fn principal_score(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() != Some(&"info") {
        return None;
    }
    let multipv = tokens.iter().position(|&t| t == "multipv");
    if multipv.is_some_and(|i| tokens.get(i + 1) != Some(&"1")) {
        return None;
    }
    let at = tokens.iter().position(|&t| t == "score")?;
    match tokens.get(at + 1..at + 3)? {
        [kind @ ("cp" | "mate"), value] => Some(format!("{kind} {value}")),
        _ => None,
    }
}

/// Writes one line to our stdout, which both threads share.
fn say(line: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        (0.0..=1.0).contains(&args.blunder),
        "--blunder must be between 0 and 1"
    );
    backrank::attacks::init();

    let mut stockfish = Command::new(&args.stockfish)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {}", args.stockfish.display()))?;
    let mut to_stockfish = stockfish.stdin.take().expect("stdin is piped");
    let from_stockfish = stockfish.stdout.take().expect("stdout is piped");
    // Everything Stockfish says goes straight to the GUI, with two exceptions. Its
    // `Skill Level` option is hidden, because the proxy sets it. And the output of the
    // proxy's own searches (see below) goes to the main thread instead. Stockfish answers
    // in the order it was asked, so a queue of pending requests says whose each reply is.
    let pending = Arc::new(Mutex::new(VecDeque::new()));
    let (own_output, own_lines) = mpsc::channel();
    let forwarder = {
        let pending = Arc::clone(&pending);
        std::thread::spawn(move || {
            for line in BufReader::new(from_stockfish).lines() {
                let Ok(line) = line else { break };
                let mut queue = pending.lock().unwrap();
                if queue.front() == Some(&Request::OwnSearch) {
                    if line.starts_with("bestmove") {
                        queue.pop_front();
                    }
                    drop(queue);
                    let _ = own_output.send(line);
                    continue;
                }
                if line.trim() == "readyok" {
                    queue.pop_front();
                }
                drop(queue);
                if !line.starts_with("option name Skill Level ") {
                    say(&line);
                }
            }
        })
    };
    writeln!(to_stockfish, "setoption name Skill Level value 0")?;

    let mut rng = Rng(args
        .seed
        .unwrap_or_else(|| RandomState::new().hash_one(std::process::id())));
    let mut position = Some(Position::startpos());
    for line in io::stdin().lock().lines() {
        let line = line?;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.first() {
            // Track the position so random moves are legal. If it can't be parsed,
            // Stockfish answers every `go` instead.
            Some(&"position") => {
                position = uci::parse_position(&tokens[1..])
                    .inspect_err(|e| {
                        eprintln!(
                            "sfproxy: can't follow `{line}` ({e}); Stockfish plays every move"
                        );
                    })
                    .ok();
            }
            Some(&"isready") => pending.lock().unwrap().push_back(Request::GuiIsReady),
            // The proxy owns Skill Level.
            Some(&"setoption") if line.contains("name Skill Level") => continue,
            Some(&"ucinewgame") => position = Some(Position::startpos()),
            Some(&"go") => {
                if let Some(pos) = position.as_mut()
                    && rng.chance(args.blunder)
                {
                    let moves = movegen::legal_moves(pos);
                    if !moves.is_empty() {
                        // A quick search gives an honest score to report with the random
                        // move (match runners read it, for example for adjudication). Waiting
                        // for it also lets Stockfish finish anything it still had to say, so
                        // the GUI sees replies in order.
                        pending.lock().unwrap().push_back(Request::OwnSearch);
                        writeln!(to_stockfish, "go depth 1")?;
                        to_stockfish.flush()?;
                        let mut score = None;
                        loop {
                            let reply = own_lines.recv().context("Stockfish stopped responding")?;
                            if let Some(s) = principal_score(&reply) {
                                score = Some(s);
                            }
                            if reply.starts_with("bestmove") {
                                break;
                            }
                        }
                        let m = moves[rng.below(moves.len())];
                        let score = score.unwrap_or_else(|| "cp 0".to_string());
                        say(&format!("info depth 1 score {score} pv {m}"));
                        say(&format!("bestmove {m}"));
                        continue;
                    }
                }
            }
            Some(&"quit") => break,
            _ => {}
        }
        writeln!(to_stockfish, "{line}")?;
        to_stockfish.flush()?;
    }
    writeln!(to_stockfish, "quit")?;
    drop(to_stockfish);
    stockfish.wait()?;
    let _ = forwarder.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_principal_score() {
        let line = "info depth 1 seldepth 2 multipv 1 score cp 31 nodes 20 pv e7e5";
        assert_eq!(principal_score(line).as_deref(), Some("cp 31"));
        let mate = "info depth 3 score mate -2 nodes 90 pv h7h6";
        assert_eq!(principal_score(mate).as_deref(), Some("mate -2"));
        let second = "info depth 1 multipv 2 score cp 12 pv d7d5";
        assert_eq!(principal_score(second), None);
        assert_eq!(
            principal_score("info string NNUE evaluation using nn.nnue"),
            None
        );
        assert_eq!(principal_score("bestmove e7e5"), None);
    }
}
