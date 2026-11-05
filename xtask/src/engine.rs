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

/// Builds the `sfproxy` helper (see `src/bin/sfproxy.rs`) and returns its path.
pub fn build_sfproxy() -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--profile",
            "tools",
            "--package",
            "xtask",
            "--bin",
            "sfproxy",
        ])
        .current_dir(paths::root())
        .status()
        .context("failed to run cargo build")?;
    ensure!(status.success(), "building sfproxy failed");
    Ok(paths::root().join("target/tools/sfproxy"))
}

/// A label for the working tree's version: the short commit hash, plus
/// `-dirty-<hash of the diff>` if tracked files have uncommitted changes, so different
/// uncommitted experiments get different labels. Changes under `results/` don't count:
/// recording a result doesn't change the engine.
pub fn version_label() -> Result<String> {
    use sha2::{Digest, Sha256};
    let sha = git(&["rev-parse", "--short", "HEAD"])?;
    let diff = git(&["diff", "HEAD", "--", ".", ":(exclude)results"])?;
    if diff.is_empty() {
        return Ok(sha);
    }
    let digest = Sha256::digest(diff.as_bytes());
    let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Ok(format!("{sha}-dirty-{short}"))
}

/// Writes a file by writing a temporary file next to it and renaming it into place, so an
/// interruption never leaves a half-written file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Runs git in the repository and returns its trimmed output.
pub fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(paths::root())
        .output()
        .context("failed to run git")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Copies an engine binary to `tools/engines/backrank-<label>`, so a match keeps using the
/// same build even if the code is rebuilt while it runs.
pub fn snapshot(binary: &Path, label: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(paths::engines())?;
    let dest = paths::engines().join(format!("backrank-{label}"));
    // Copy then rename: an interrupted copy never leaves a truncated binary under the
    // real name, and a binary that is running is never overwritten in place.
    let tmp = dest.with_extension("tmp");
    std::fs::copy(binary, &tmp)
        .with_context(|| format!("failed to copy {} to {}", binary.display(), tmp.display()))?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
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
