//! Building Backrank, and minimal UCI client helpers for talking to engines outside of
//! fastchess.

use crate::paths;
use anyhow::{Context, Result, bail, ensure};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Builds the release `backrank` binary and returns its path.
pub fn build_backrank() -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args([
            "build",
            "--release",
            "--quiet",
            "--package",
            "backrank",
            "--bin",
            "backrank",
        ])
        .current_dir(paths::root())
        .status()
        .context("failed to run cargo build")?;
    ensure!(status.success(), "building backrank failed");
    Ok(paths::root().join("target/release/backrank"))
}

/// Runs the UCI handshake and returns the engine's `id name` (e.g. "Stockfish 19").
pub fn id_name(engine: &Path) -> Result<String> {
    let mut child = Command::new(engine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start {}", engine.display()))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(b"uci\nquit\n")?;
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout
        .lines()
        .find_map(|line| line.strip_prefix("id name "))
    {
        Some(name) => Ok(name.trim().to_string()),
        None => bail!("{} did not report `id name` to `uci`", engine.display()),
    }
}
