//! Building and running fastchess command lines.
//!
//! Arguments are passed straight to the process (no shell), so UCI option names that
//! contain spaces, such as `Skill Level`, need no quoting. fastchess splits each
//! `key=value` argument at its first `=`.

use crate::paths;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One engine in a match.
pub struct EngineSpec {
    pub name: String,
    pub cmd: PathBuf,
    /// UCI options sent with `setoption`, in order.
    pub options: Vec<(String, String)>,
}

impl EngineSpec {
    pub fn new(name: impl Into<String>, cmd: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            cmd: cmd.into(),
            options: Vec::new(),
        }
    }

    pub fn option(mut self, name: &str, value: &str) -> Self {
        self.options.push((name.to_string(), value.to_string()));
        self
    }

    fn push_args(&self, args: &mut Vec<String>) {
        args.push("-engine".into());
        args.push(format!("cmd={}", self.cmd.display()));
        args.push(format!("name={}", self.name));
        for (name, value) in &self.options {
            args.push(format!("option.{name}={value}"));
        }
    }
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
        args
    }

    /// Runs fastchess with its output streamed to the terminal.
    pub fn run(&self) -> Result<()> {
        let fastchess = paths::fastchess();
        let logs = paths::results_logs();
        std::fs::create_dir_all(&logs)?;
        std::fs::create_dir_all(paths::results_pgn())?;
        let status = Command::new(&fastchess)
            .args(self.args())
            .current_dir(&logs)
            .status()
            .with_context(|| format!("failed to start {}", fastchess.display()))?;
        if !status.success() {
            bail!("fastchess exited with {status}; see {}", self.log.display());
        }
        Ok(())
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
    }
}
