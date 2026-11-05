//! UCI protocol tests: these run the real `backrank` binary and talk to it over pipes,
//! as a GUI would.

use backrank::{movegen, uci};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Long enough for any single reply; a test fails instead of hanging if one never comes.
const TIMEOUT: Duration = Duration::from_secs(10);

struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Engine {
    fn start() -> Engine {
        let mut child = Command::new(env!("CARGO_BIN_EXE_backrank"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start backrank");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Engine {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, command: &str) {
        self.send_bytes(format!("{command}\n").as_bytes());
    }

    fn send_bytes(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }

    /// Sends `isready` and returns everything printed before the `readyok`. The engine
    /// handles commands in order, so earlier commands have been dealt with by then.
    fn sync(&mut self) -> Vec<String> {
        self.send("isready");
        let mut lines = self.read_until(|l| l == "readyok");
        lines.pop();
        lines
    }

    /// Reads lines up to and including the first one for which `done` is true.
    fn read_until(&self, done: impl Fn(&str) -> bool) -> Vec<String> {
        let deadline = Instant::now() + TIMEOUT;
        let mut lines = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let finished = done(&line);
                    lines.push(line);
                    if finished {
                        return lines;
                    }
                }
                Err(RecvTimeoutError::Timeout) => panic!("timed out; got {lines:?}"),
                Err(RecvTimeoutError::Disconnected) => panic!("engine exited; got {lines:?}"),
            }
        }
    }

    /// Everything the engine prints during `wait`.
    fn read_for(&self, wait: Duration) -> Vec<String> {
        let deadline = Instant::now() + wait;
        let mut lines = Vec::new();
        while let Ok(line) = self
            .lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            lines.push(line);
        }
        lines
    }

    /// Sends `go ...` and returns the move from the `bestmove` line.
    fn best_move(&mut self, go: &str) -> String {
        self.send(go);
        let lines = self.read_until(|l| l.starts_with("bestmove"));
        bestmove_of(lines.last().unwrap())
    }

    /// Waits for the process to exit and returns whether it did so successfully.
    fn wait_for_exit(&mut self) -> bool {
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        panic!("engine did not exit");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn bestmove_of(line: &str) -> String {
    let mut tokens = line.split_whitespace();
    assert_eq!(tokens.next(), Some("bestmove"), "{line}");
    tokens.next().expect("bestmove without a move").to_string()
}

/// Asserts that `mv` is legal after the given `position` arguments.
fn assert_legal(position: &str, mv: &str) {
    let tokens: Vec<&str> = position.split_whitespace().collect();
    let mut pos = uci::parse_position(&tokens).unwrap();
    assert!(
        movegen::find_legal_move(&mut pos, mv).is_some(),
        "{mv} is not legal after `position {position}`"
    );
}

#[test]
fn handshake_identifies_the_engine_and_its_options() {
    let mut engine = Engine::start();
    engine.send("uci");
    let lines = engine.read_until(|l| l == "uciok");
    assert!(lines[0].starts_with("id name Backrank "), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("id author ")));
    assert!(
        lines
            .contains(&"option name Move Overhead type spin default 20 min 0 max 5000".to_string())
    );
    engine.send("isready");
    assert_eq!(engine.read_until(|l| l == "readyok"), ["readyok"]);
}

#[test]
fn plays_legal_moves_from_startpos_and_fen() {
    let mut engine = Engine::start();
    for position in [
        "startpos",
        "startpos moves e2e4 e7e5 g1f3",
        "fen r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "fen 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1 moves c7c5",
    ] {
        engine.send(&format!("position {position}"));
        for go in [
            "go depth 1",
            "go movetime 50",
            "go wtime 1000 btime 1000 winc 10 binc 10",
            "go btime -20 wtime -20",
            "go nodes 100",
        ] {
            let mv = engine.best_move(go);
            assert_legal(position, &mv);
        }
    }
}

#[test]
fn infinite_search_waits_for_stop_and_answers_isready_meanwhile() {
    let mut engine = Engine::start();
    engine.send("position startpos");
    engine.send("go infinite");
    // `readyok` must come while the search runs. Info lines may arrive before or after it.
    let mut early = engine.sync();
    early.extend(engine.read_for(Duration::from_millis(200)));
    assert!(
        early.iter().all(|l| !l.starts_with("bestmove")),
        "bestmove before stop: {early:?}"
    );
    engine.send("stop");
    let lines = engine.read_until(|l| l.starts_with("bestmove"));
    assert_legal("startpos", &bestmove_of(lines.last().unwrap()));
    // Exactly one bestmove, and stopping when idle prints nothing.
    engine.send("stop");
    assert_eq!(engine.sync(), Vec::<String>::new());
}

#[test]
fn bare_go_is_infinite() {
    let mut engine = Engine::start();
    engine.send("go");
    assert!(
        engine
            .read_for(Duration::from_millis(150))
            .iter()
            .all(|l| !l.starts_with("bestmove"))
    );
    engine.send("stop");
    engine.read_until(|l| l.starts_with("bestmove"));
}

#[test]
fn no_legal_move_gives_null_bestmove() {
    let mut engine = Engine::start();
    engine.send("position fen 7k/6Q1/6K1/8/8/8/8/8 b - - 0 1");
    engine.send("go depth 1");
    let lines = engine.read_until(|l| l.starts_with("bestmove"));
    assert_eq!(lines.last().unwrap(), "bestmove 0000");
    assert!(lines[0].contains("score mate 0"), "{lines:?}");
}

#[test]
fn reports_an_info_line_with_a_score_before_bestmove() {
    let mut engine = Engine::start();
    engine.send("go wtime 100 btime 100");
    let lines = engine.read_until(|l| l.starts_with("bestmove"));
    let info = lines
        .iter()
        .rfind(|l| l.starts_with("info "))
        .unwrap_or_else(|| panic!("no info line in {lines:?}"));
    assert!(
        info.contains(" score cp ") && info.contains(" depth "),
        "{info}"
    );
    let pv = info.split(" pv ").nth(1).unwrap();
    let first = pv.split_whitespace().next().unwrap();
    assert_eq!(first, bestmove_of(lines.last().unwrap()));
}

#[test]
fn invalid_position_is_reported_and_the_old_one_kept() {
    let mut engine = Engine::start();
    engine.send("position startpos moves e2e4");
    engine.send("position fen not/a/fen w - - 0 1");
    let reply = engine.read_until(|l| l.starts_with("info string"));
    assert!(
        reply[0].contains("keeping the previous position"),
        "{reply:?}"
    );
    engine.send("position startpos moves e2e4 e2e4");
    let reply = engine.read_until(|l| l.starts_with("info string"));
    assert!(reply[0].contains("illegal move e2e4"), "{reply:?}");
    engine.send("d");
    let board = engine.read_until(|l| l.starts_with("Key:"));
    assert!(
        board.contains(
            &"FEN: rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".to_string()
        ),
        "{board:?}"
    );
}

#[test]
fn ucinewgame_resets_to_the_start_position() {
    let mut engine = Engine::start();
    engine.send("position startpos moves d2d4");
    engine.send("ucinewgame");
    engine.send("d");
    let board = engine.read_until(|l| l.starts_with("Key:"));
    assert!(
        board.iter().any(|l| l.contains("RNBQKBNR w KQkq - 0 1")),
        "{board:?}"
    );
}

#[test]
fn options_are_validated() {
    let mut engine = Engine::start();
    engine.send("setoption name Move Overhead value 100");
    engine.send("setoption name move overhead value 9999");
    let reply = engine.read_until(|l| l.starts_with("info string"));
    assert!(
        reply[0].contains("Move Overhead needs a value"),
        "{reply:?}"
    );
    engine.send("setoption name Hash value 64");
    let reply = engine.read_until(|l| l.starts_with("info string"));
    assert!(reply[0].contains("unknown option: Hash"), "{reply:?}");
}

#[test]
fn garbage_is_ignored() {
    let mut engine = Engine::start();
    for junk in [
        "",
        "   ",
        "hello there",
        "position",
        "go wtime",
        "setoption",
        "\u{1f600}",
    ] {
        engine.send(junk);
    }
    engine.send("isready");
    engine.read_until(|l| l == "readyok");
    // `position` with no arguments means the start position.
    assert_legal("startpos", &engine.best_move("go depth 1"));
}

#[test]
fn go_perft_matches_known_counts() {
    let mut engine = Engine::start();
    engine.send("position startpos");
    engine.send("go perft 3");
    let lines = engine.read_until(|l| l.starts_with("Nodes searched"));
    assert_eq!(lines.last().unwrap(), "Nodes searched: 8902");
    assert_eq!(lines.iter().filter(|l| l.contains(": ")).count(), 21);
}

#[test]
fn quit_exits_promptly_even_while_searching() {
    let mut engine = Engine::start();
    engine.send("go infinite");
    engine.send("quit");
    assert!(engine.wait_for_exit());
}

#[test]
fn end_of_input_exits() {
    let mut engine = Engine::start();
    engine.send("isready");
    engine.read_until(|l| l == "readyok");
    drop(engine.stdin.take());
    assert!(engine.wait_for_exit());
}

#[test]
fn invalid_utf8_input_does_not_end_the_session() {
    let mut engine = Engine::start();
    engine.send_bytes(b"setoption name Book\xff value \xfe\n");
    let before_ready = engine.sync();
    assert!(
        before_ready[0].contains("unknown option"),
        "{before_ready:?}"
    );
    assert_legal("startpos", &engine.best_move("go depth 1"));
}

#[test]
fn exactly_one_bestmove_per_go() {
    let mut engine = Engine::start();
    engine.send("go depth 1");
    engine.send("stop");
    engine.send("go movetime 50");
    engine.send("stop");
    engine.send("stop");
    let lines = engine.sync();
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("bestmove")).count(),
        2,
        "{lines:?}"
    );
}

#[test]
fn a_new_position_mid_search_ends_the_old_search_first() {
    let mut engine = Engine::start();
    engine.send("go infinite");
    engine.send("position startpos moves e2e4");
    let lines = engine.sync();
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("bestmove")).count(),
        1
    );
    assert_legal(
        "startpos",
        &bestmove_of(lines.iter().rfind(|l| l.starts_with("bestmove")).unwrap()),
    );
    assert_legal("startpos moves e2e4", &engine.best_move("go depth 1"));
}

#[test]
fn a_clock_for_the_other_side_only_means_search_until_stop() {
    let mut engine = Engine::start();
    engine.send("position startpos moves e2e4");
    engine.send("go wtime 1000");
    let early = engine.sync();
    assert!(
        early.iter().all(|l| !l.starts_with("bestmove")),
        "{early:?}"
    );
    engine.send("stop");
    let lines = engine.read_until(|l| l.starts_with("bestmove"));
    assert_legal("startpos moves e2e4", &bestmove_of(lines.last().unwrap()));
}

#[test]
fn perft_runs_in_the_background_and_can_be_stopped() {
    let mut engine = Engine::start();
    engine.send("go perft 0");
    let reply = engine.read_until(|l| l.starts_with("info string"));
    assert!(reply[0].contains("depth of at least 1"), "{reply:?}");
    // Far too deep to finish: the engine must still answer, and stop it on request.
    engine.send("go perft 12");
    assert!(engine.sync().is_empty());
    engine.send("stop");
    let stopped = engine.read_until(|l| l.starts_with("info string"));
    assert_eq!(stopped.last().unwrap(), "info string perft stopped");
    engine.send("go perft 12");
    engine.send("quit");
    assert!(engine.wait_for_exit());
}

#[test]
fn end_of_input_during_search_still_sends_bestmove() {
    let mut engine = Engine::start();
    engine.send("go infinite");
    engine.sync();
    drop(engine.stdin.take());
    let lines = engine.read_until(|l| l.starts_with("bestmove"));
    assert_legal("startpos", &bestmove_of(lines.last().unwrap()));
    assert!(engine.wait_for_exit());
}
