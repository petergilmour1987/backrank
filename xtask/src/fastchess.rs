//! Building and running fastchess command lines.
//!
//! Arguments are passed straight to the process (no shell), so UCI option names that
//! contain spaces, such as `Skill Level`, need no quoting. fastchess splits each
//! `key=value` argument at its first `=`.

use crate::{paths, pgn};
use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One engine in a match.
pub struct EngineSpec {
    pub name: String,
    pub cmd: PathBuf,
    /// Command-line arguments for the engine. fastchess splits them on spaces, so none may
    /// contain one.
    pub args: Vec<String>,
    /// UCI options sent with `setoption`, in order.
    pub options: Vec<(String, String)>,
}

impl EngineSpec {
    pub fn new(name: impl Into<String>, cmd: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            cmd: cmd.into(),
            args: Vec::new(),
            options: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn option(mut self, name: &str, value: &str) -> Self {
        self.options.push((name.to_string(), value.to_string()));
        self
    }

    fn push_args(&self, args: &mut Vec<String>) {
        args.push("-engine".into());
        args.push(format!("cmd={}", self.cmd.display()));
        args.push(format!("name={}", self.name));
        if !self.args.is_empty() {
            args.push(format!("args={}", self.args.join(" ")));
        }
        for (name, value) in &self.options {
            args.push(format!("option.{name}={value}"));
        }
    }
}

/// Sequential probability ratio test bounds, in normalized Elo (fastchess's default model).
pub struct Sprt {
    pub elo0: f64,
    pub elo1: f64,
    pub alpha: f64,
    pub beta: f64,
}

/// A two-engine match. Every opening is played twice with colours swapped.
pub struct Match<'a> {
    pub engines: [EngineSpec; 2],
    /// Cute Chess time control format, e.g. `8+0.08`.
    pub tc: &'a str,
    /// Each round is one opening played from both sides (two games).
    pub rounds: u32,
    pub concurrency: u32,
    pub book: &'a Path,
    pub pgn_out: &'a Path,
    pub log: &'a Path,
    /// Seed for the random opening order, so a run can be repeated exactly.
    pub seed: Option<u64>,
    /// Stop as soon as this test reaches a verdict, instead of after `rounds`.
    pub sprt: Option<Sprt>,
}

impl Match<'_> {
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for engine in &self.engines {
            engine.push_args(&mut args);
        }
        let book = self.book.display();
        let pgn = self.pgn_out.display();
        let log = self.log.display();
        args.extend([
            "-each".into(),
            format!("tc={}", self.tc),
            "-openings".into(),
            format!("file={book}"),
            "format=epd".into(),
            "order=random".into(),
            "-rounds".into(),
            self.rounds.to_string(),
            "-repeat".into(),
            "-concurrency".into(),
            self.concurrency.to_string(),
            "-pgnout".into(),
            format!("file={pgn}"),
            "-log".into(),
            format!("file={log}"),
            "level=warn".into(),
            // No periodic resume snapshots. fastchess still writes config.json when the
            // match ends; `run` sets the working directory to results/logs to keep it there.
            "-autosaveinterval".into(),
            "0".into(),
        ]);
        if let Some(seed) = self.seed {
            args.extend(["-srand".into(), seed.to_string()]);
        }
        if let Some(sprt) = &self.sprt {
            args.extend([
                "-sprt".into(),
                format!("elo0={}", sprt.elo0),
                format!("elo1={}", sprt.elo1),
                format!("alpha={}", sprt.alpha),
                format!("beta={}", sprt.beta),
            ]);
        }
        args
    }

    /// Runs fastchess with its output streamed to the terminal.
    pub fn run(&self) -> Result<()> {
        self.run_capturing().map(|_| ())
    }

    /// Runs fastchess, streaming its output to the terminal, and returns that output.
    pub fn run_capturing(&self) -> Result<String> {
        let fastchess = paths::fastchess();
        let logs = paths::results_logs();
        std::fs::create_dir_all(&logs)?;
        std::fs::create_dir_all(paths::results_pgn())?;
        let mut child = Command::new(&fastchess)
            .args(self.args())
            .current_dir(&logs)
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start {}", fastchess.display()))?;
        let mut output = String::new();
        for line in BufReader::new(child.stdout.take().expect("stdout is piped")).lines() {
            let line = line?;
            println!("{line}");
            output.push_str(&line);
            output.push('\n');
        }
        let status = child.wait()?;
        if !status.success() {
            bail!("fastchess exited with {status}; see {}", self.log.display());
        }
        Ok(output)
    }

    /// Reads back the games this match wrote, tallied for `engine` against `opponent`.
    pub fn tally(&self, engine: &str, opponent: &str) -> Result<pgn::Tally> {
        let text = std::fs::read_to_string(self.pgn_out)
            .with_context(|| format!("fastchess wrote no PGN at {}", self.pgn_out.display()))?;
        Ok(pgn::Tally::between(&pgn::parse(&text)?, engine, opponent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_engine_and_match_args() {
        let m = Match {
            engines: [
                EngineSpec::new("weak", "/sf").option("Skill Level", "0"),
                EngineSpec::new("strong", "/sf"),
            ],
            tc: "2+0.02",
            rounds: 10,
            concurrency: 4,
            book: Path::new("/book.epd"),
            pgn_out: Path::new("/out.pgn"),
            log: Path::new("/log.txt"),
            seed: Some(7),
            sprt: None,
        };
        let args = m.args();
        let expected_start = [
            "-engine",
            "cmd=/sf",
            "name=weak",
            "option.Skill Level=0",
            "-engine",
            "cmd=/sf",
            "name=strong",
            "-each",
            "tc=2+0.02",
        ];
        assert_eq!(&args[..expected_start.len()], expected_start);
        let joined = args.join(" ");
        assert!(joined.contains("-rounds 10 -repeat -concurrency 4"));
        assert!(joined.contains("-openings file=/book.epd format=epd order=random"));
        assert!(joined.ends_with("-srand 7"));
    }

    #[test]
    fn engine_arguments_and_sprt() {
        let mut args = Vec::new();
        EngineSpec::new("proxy", "/bin/sfproxy")
            .arg("--blunder")
            .arg("0.5")
            .push_args(&mut args);
        assert_eq!(
            args,
            [
                "-engine",
                "cmd=/bin/sfproxy",
                "name=proxy",
                "args=--blunder 0.5"
            ]
        );
        let m = Match {
            engines: [EngineSpec::new("a", "/a"), EngineSpec::new("b", "/b")],
            tc: "8+0.08",
            rounds: 100,
            concurrency: 8,
            book: Path::new("/book.epd"),
            pgn_out: Path::new("/out.pgn"),
            log: Path::new("/log.txt"),
            seed: None,
            sprt: Some(Sprt {
                elo0: 0.0,
                elo1: 10.0,
                alpha: 0.05,
                beta: 0.05,
            }),
        };
        assert!(
            m.args()
                .join(" ")
                .ends_with("-sprt elo0=0 elo1=10 alpha=0.05 beta=0.05")
        );
    }
}
