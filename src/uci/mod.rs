//! The UCI protocol front end.
//!
//! Three threads are involved. A reader thread turns lines from stdin into [`Command`]s
//! and sends them to the engine thread over a channel. The engine thread owns the position
//! and answers commands; it never waits on a search, so `isready` is answered at once even
//! mid-search. Each `go` starts a search thread with its own copy of the position and a
//! shared stop flag; that thread prints the `bestmove`, so every `go` gets exactly one.

mod command;
mod position;

pub use command::{Command, GoParams, parse};
pub use position::{PositionError, parse_position};

use crate::board::Position;
use crate::perft;
use crate::search::{self, Info, Limits, Score};
use crate::types::{Color, Move};
use crate::{ENGINE_AUTHOR, ENGINE_NAME, ENGINE_VERSION};
use std::fmt::Display;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_MOVE_OVERHEAD_MS: u64 = 20;
const MAX_MOVE_OVERHEAD_MS: u64 = 5000;

/// Stack size for the search thread. A recursive search keeps a move list per ply on the
/// stack, more than the default 2 MiB allows for.
const SEARCH_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Runs the UCI loop on stdin and stdout until `quit` or the end of input.
pub fn run() {
    let (sender, commands) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match stdin.read_until(b'\n', &mut bytes) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
            // Decode leniently: a stray non-UTF-8 byte (in a file path, say) must not end
            // the session.
            if let Some(command) = parse(&String::from_utf8_lossy(&bytes))
                && sender.send(command).is_err()
            {
                return;
            }
        }
        // The GUI closed our input: treat it as `quit`.
        let _ = sender.send(Command::Quit);
    });

    let mut engine = Engine::new();
    for command in commands {
        if !engine.handle(command) {
            break;
        }
    }
    engine.stop_search();
}

/// Writes one line to the GUI and flushes it. The main and search threads both use this;
/// stdout's lock keeps their lines from interleaving.
fn send(line: impl Display) {
    let mut out = io::stdout().lock();
    // If the GUI has gone away there is nobody to report the error to.
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// An `info` line: `info depth D score (cp X | mate N) nodes N nps N time T [pv ...]`.
fn format_info(info: &Info) -> String {
    let score = match info.score {
        Score::Centipawns(cp) => format!("cp {cp}"),
        Score::Mate(moves) => format!("mate {moves}"),
    };
    let ms = info.time.as_millis();
    let nps = u128::from(info.nodes) * 1000 / ms.max(1);
    let mut line = format!(
        "info depth {} score {score} nodes {} nps {nps} time {ms}",
        info.depth, info.nodes
    );
    if !info.pv.is_empty() {
        line.push_str(" pv");
        for m in &info.pv {
            line.push_str(&format!(" {m}"));
        }
    }
    line
}

/// Non-standard `go perft <depth>`, in Stockfish's output format. Stops early, without a
/// total, if `stop` is set.
fn perft_command(position: &mut Position, depth: u32, stop: &AtomicBool) {
    match perft::divide_until_stopped(position, depth, stop) {
        Some(split) => {
            for (m, nodes) in &split {
                send(format!("{m}: {nodes}"));
            }
            send("");
            send(format!(
                "Nodes searched: {}",
                split.iter().map(|&(_, n)| n).sum::<u64>()
            ));
        }
        None => send("info string perft stopped"),
    }
}

struct SearchThread {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

struct Engine {
    position: Position,
    search: Option<SearchThread>,
    move_overhead: Duration,
}

impl Engine {
    fn new() -> Engine {
        Engine {
            position: Position::startpos(),
            search: None,
            move_overhead: Duration::from_millis(DEFAULT_MOVE_OVERHEAD_MS),
        }
    }

    /// Handles one command. Returns `false` when the engine should exit.
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Uci => {
                send(format!("id name {ENGINE_NAME} {ENGINE_VERSION}"));
                send(format!("id author {ENGINE_AUTHOR}"));
                send(format!(
                    "option name Move Overhead type spin default {DEFAULT_MOVE_OVERHEAD_MS} \
                     min 0 max {MAX_MOVE_OVERHEAD_MS}"
                ));
                send("uciok");
            }
            Command::IsReady => send("readyok"),
            Command::SetOption { name, value } => self.set_option(&name, value.as_deref()),
            Command::UciNewGame => {
                self.stop_search();
                self.position = Position::startpos();
            }
            Command::Position(tokens) => self.set_position(&tokens),
            Command::Go(params) => self.go(&params),
            Command::Stop => self.stop_search(),
            Command::Quit => {
                self.stop_search();
                return false;
            }
            Command::Display => send(&self.position),
            // Pondering is not offered, so ponderhit never applies; debug and register
            // have nothing to do.
            Command::Debug(_) | Command::Register | Command::PonderHit => {}
            Command::Unknown(line) => send(format!("info string unknown command: {line}")),
        }
        true
    }

    fn set_option(&mut self, name: &str, value: Option<&str>) {
        if name.eq_ignore_ascii_case("Move Overhead") {
            match value
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&ms| ms <= MAX_MOVE_OVERHEAD_MS)
            {
                Some(ms) => self.move_overhead = Duration::from_millis(ms),
                None => send(format!(
                    "info string Move Overhead needs a value from 0 to {MAX_MOVE_OVERHEAD_MS}"
                )),
            }
        } else {
            send(format!("info string unknown option: {name}"));
        }
    }

    /// Replaces the position, or keeps the old one if the new one is invalid.
    fn set_position(&mut self, tokens: &[String]) {
        self.stop_search();
        let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
        match parse_position(&tokens) {
            Ok(position) => self.position = position,
            Err(e) => send(format!(
                "info string invalid position ({e}); keeping the previous position"
            )),
        }
    }

    fn go(&mut self, params: &GoParams) {
        // The clock is already running for us, even while a previous search winds down.
        let start = Instant::now();
        self.stop_search();
        let mut position = self.position.clone();
        if let Some(depth) = params.perft {
            if depth == 0 {
                send("info string perft needs a depth of at least 1");
                return;
            }
            self.start_worker(move |stop| perft_command(&mut position, depth, stop));
            return;
        }
        let limits = self.limits(params, start);
        self.start_worker(move |stop| {
            let best = search::search(&mut position, &limits, stop, &mut |info| {
                send(format_info(info));
            });
            send(format!("bestmove {}", best.unwrap_or(Move::NULL)));
        });
    }

    /// Runs `work` on a new search thread, which `stop_search` can stop and wait for.
    fn start_worker(&mut self, work: impl FnOnce(&AtomicBool) + Send + 'static) {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("search".to_string())
            .stack_size(SEARCH_STACK_BYTES)
            .spawn(move || work(&thread_stop))
            .expect("failed to start the search thread");
        self.search = Some(SearchThread { stop, handle });
    }

    /// Turns `go` arguments into search limits for the side to move.
    fn limits(&self, params: &GoParams, start: Instant) -> Limits {
        let ms = |v: i64| Duration::from_millis(v.max(0).unsigned_abs());
        let (time, increment) = match self.position.side_to_move() {
            Color::White => (params.wtime, params.winc),
            Color::Black => (params.btime, params.binc),
        };
        // `searchmoves` and `mate` are parsed but not supported yet.
        let mut limits = Limits {
            start,
            time_left: time.map(ms),
            increment: increment.map(ms).unwrap_or_default(),
            moves_to_go: params.movestogo,
            move_time: params.movetime.map(ms),
            depth: params.depth,
            nodes: params.nodes,
            infinite: false,
            move_overhead: self.move_overhead,
        };
        // Search until `stop` when asked to, when pondering (not offered, but a GUI may
        // ponder anyway), and when nothing else would end the search: a bare `go`,
        // `go mate N`, or a clock given only for the other side.
        limits.infinite = params.infinite || params.ponder || !limits.is_bounded();
        limits
    }

    /// Stops the running search (or perft), if any, and waits for it to finish printing.
    fn stop_search(&mut self) {
        if let Some(search) = self.search.take() {
            search.stop.store(true, Ordering::Relaxed);
            // A search thread that panicked has already aborted the process (release
            // builds use panic = "abort"), so there is nothing useful to do with an error.
            let _ = search.handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    fn limits_for(position: &str, go: &str) -> Limits {
        let mut engine = Engine::new();
        let tokens: Vec<&str> = position.split_whitespace().collect();
        engine.position = parse_position(&tokens).unwrap();
        let Some(Command::Go(params)) = parse(go) else {
            panic!("{go} is not a go command");
        };
        engine.limits(&params, Instant::now())
    }

    #[test]
    fn limits_use_the_side_to_moves_clock() {
        let white = limits_for("startpos", "go wtime 9000 btime 3000 winc 100 binc 50");
        assert_eq!(white.time_left, Some(Duration::from_millis(9000)));
        assert_eq!(white.increment, Duration::from_millis(100));
        assert!(!white.infinite);
        let black = limits_for(
            "startpos moves e2e4",
            "go wtime 9000 btime 3000 winc 100 binc 50",
        );
        assert_eq!(black.time_left, Some(Duration::from_millis(3000)));
        assert_eq!(black.increment, Duration::from_millis(50));
        // A clock that has run out is zero, not negative.
        let flagged = limits_for("startpos", "go wtime -40 btime 100");
        assert_eq!(flagged.time_left, Some(Duration::ZERO));
        assert_eq!(
            flagged.move_overhead,
            Duration::from_millis(DEFAULT_MOVE_OVERHEAD_MS)
        );
    }

    #[test]
    fn unbounded_searches_run_until_stop() {
        for (position, go) in [
            ("startpos", "go"),
            ("startpos", "go infinite"),
            ("startpos", "go mate 3"),
            ("startpos", "go movestogo 5"),
            ("startpos", "go winc 100"),
            ("startpos", "go ponder wtime 1000 btime 1000"),
            // Only white's clock, with black to move.
            ("startpos moves e2e4", "go wtime 1000"),
        ] {
            assert!(limits_for(position, go).infinite, "{position} / {go}");
        }
        for go in [
            "go depth 3",
            "go nodes 1000",
            "go movetime 100",
            "go wtime 1000",
        ] {
            assert!(!limits_for("startpos", go).infinite, "{go}");
        }
    }

    #[test]
    fn formats_info_lines() {
        let info = Info {
            depth: 3,
            score: Score::Centipawns(-25),
            nodes: 5000,
            time: Duration::from_millis(250),
            pv: vec![
                Move::quiet(Square::E2, Square::E3),
                Move::quiet(Square::E7, Square::E6),
            ],
        };
        assert_eq!(
            format_info(&info),
            "info depth 3 score cp -25 nodes 5000 nps 20000 time 250 pv e2e3 e7e6"
        );
        let mated = Info {
            depth: 0,
            score: Score::Mate(0),
            nodes: 0,
            time: Duration::ZERO,
            pv: Vec::new(),
        };
        assert_eq!(
            format_info(&mated),
            "info depth 0 score mate 0 nodes 0 nps 0 time 0"
        );
    }
}
