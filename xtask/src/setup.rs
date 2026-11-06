//! `cargo xtask setup`: install the match tooling into `tools/`.
//!
//! Downloads are pinned to a version or commit and checked against a SHA-256 digest, so
//! every machine runs the same fastchess and the same openings.

use crate::{engine, paths};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const FASTCHESS_VERSION: &str = "v1.8.2-alpha";

/// (OS, arch, release asset stem, SHA-256 of `<stem>.tar` as published by GitHub).
const FASTCHESS_ASSETS: &[(&str, &str, &str, &str)] = &[
    (
        "macos",
        "aarch64",
        "fastchess-mac-arm64",
        "a736371b1df3d7c67d4288dee66f6bb4e7e8193a945639c8a1d0aabfa612b551",
    ),
    (
        "macos",
        "x86_64",
        "fastchess-mac-x86-64",
        "c97c62cdfc52449b901238c7326de5e416434f0f98c979b6e16129c5d41affcc",
    ),
    (
        "linux",
        "x86_64",
        "fastchess-linux-x86-64",
        "1003f920bebe841acdab5e6d7871e93171a4586857b3ec91c21ef5777f26ff96",
    ),
    (
        "linux",
        "aarch64",
        "fastchess-linux-arm64",
        "fe079f8784394488baa199d4196768e46ccab0b31741d67c9cd70db0a237a494",
    ),
];

/// Commit of https://github.com/official-stockfish/books the opening book is taken from.
const BOOK_COMMIT: &str = "77ba28be2480f3b26a915b43b924841e107212fa";
const BOOK_ZIP: &str = "noob_3moves.epd.zip";
const BOOK_SHA256: &str = "68fe2691e0876d0de54fffb68c797e752138d5ed363beab2ac6ca34be68ee9f3";

#[derive(clap::Args)]
pub struct Args {
    /// Re-install everything, even files that already exist.
    #[arg(long)]
    force: bool,
    /// Stockfish binary to snapshot: a path, or a command name looked up on PATH.
    #[arg(long, default_value = "stockfish")]
    stockfish: String,
    /// Skip the Stockfish snapshot, for machines that only run Backrank against itself,
    /// such as CI.
    #[arg(long, conflicts_with = "stockfish")]
    skip_stockfish: bool,
}

pub fn run(args: &Args) -> Result<()> {
    install_fastchess(args.force)?;
    install_book(args.force)?;
    if !args.skip_stockfish {
        snapshot_stockfish(&args.stockfish, args.force)?;
    }
    println!("setup complete");
    Ok(())
}

fn install_fastchess(force: bool) -> Result<()> {
    let dest = paths::fastchess();
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let Some(&(_, _, stem, sha256)) = FASTCHESS_ASSETS
        .iter()
        .find(|(o, a, _, _)| *o == os && *a == arch)
    else {
        bail!("no prebuilt fastchess for {os}/{arch}; build it from source into tools/bin/");
    };
    if !force && is_installed(&dest, sha256) {
        println!(
            "fastchess: {FASTCHESS_VERSION} already installed at {}",
            dest.display()
        );
        return Ok(());
    }

    let bin_dir = paths::tools_bin();
    std::fs::create_dir_all(&bin_dir)?;
    let tar = bin_dir.join(format!("{stem}.tar"));
    let url = format!(
        "https://github.com/Disservin/fastchess/releases/download/{FASTCHESS_VERSION}/{stem}.tar"
    );
    download(&url, &tar, sha256)?;

    let unpack = fresh_dir(&bin_dir.join(".unpack"))?;
    run_tool(
        Command::new("tar")
            .arg("-xf")
            .arg(&tar)
            .arg("-C")
            .arg(&unpack),
    )?;
    std::fs::rename(unpack.join(stem).join("fastchess"), &dest)
        .context("fastchess binary not found in the release archive")?;
    std::fs::remove_dir_all(&unpack)?;
    std::fs::remove_file(&tar)?;

    let version = Command::new(&dest)
        .arg("-version")
        .output()
        .with_context(|| format!("installed fastchess at {} does not run", dest.display()))?;
    mark_installed(&dest, sha256)?;
    println!(
        "fastchess: installed {} ({})",
        dest.display(),
        String::from_utf8_lossy(&version.stdout).trim()
    );
    Ok(())
}

fn install_book(force: bool) -> Result<()> {
    let dest = paths::book();
    if !force && is_installed(&dest, BOOK_SHA256) {
        println!("book: already installed at {}", dest.display());
        return Ok(());
    }
    let dir = paths::books();
    std::fs::create_dir_all(&dir)?;
    let zip = dir.join(BOOK_ZIP);
    let url = format!(
        "https://raw.githubusercontent.com/official-stockfish/books/{BOOK_COMMIT}/{BOOK_ZIP}"
    );
    download(&url, &zip, BOOK_SHA256)?;

    // Extract beside the destination and rename, so an interrupted unzip never leaves a
    // truncated book at the real path.
    let unpack = fresh_dir(&dir.join(".unpack"))?;
    run_tool(
        Command::new("unzip")
            .args(["-o", "-q"])
            .arg(&zip)
            .arg("-d")
            .arg(&unpack),
    )?;
    let file_name = dest.file_name().expect("book path has a file name");
    std::fs::rename(unpack.join(file_name), &dest)
        .with_context(|| format!("{BOOK_ZIP} did not contain {}", file_name.display()))?;
    std::fs::remove_dir_all(&unpack)?;
    std::fs::remove_file(&zip)?;

    let positions = std::fs::read_to_string(&dest)?.lines().count();
    mark_installed(&dest, BOOK_SHA256)?;
    println!("book: installed {} ({positions} positions)", dest.display());
    Ok(())
}

/// Copies Stockfish into `tools/engines/` so a package-manager upgrade can't silently
/// change the opponent between runs.
fn snapshot_stockfish(source: &str, force: bool) -> Result<()> {
    let dest = paths::stockfish();
    if dest.exists() && !force {
        match engine::id_name(&dest) {
            Ok(name) => {
                println!("stockfish: snapshot already at {} ({name})", dest.display());
                return Ok(());
            }
            Err(err) => println!("stockfish: existing snapshot is broken ({err:#}); replacing it"),
        }
    }
    let source = resolve_command(source)?;
    std::fs::create_dir_all(paths::engines())?;

    // Copy to a temporary name, check it runs, then rename over the old snapshot, so an
    // interrupted copy never leaves a broken binary at the real path. The copy keeps the
    // source's permissions (Homebrew's binary is read-only), so clear out any leftover
    // temporary file instead of overwriting it. fs::copy follows symlinks.
    let tmp = dest.with_extension("tmp");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    std::fs::copy(&source, &tmp).with_context(|| format!("failed to copy {}", source.display()))?;
    let name = engine::id_name(&tmp)?;
    std::fs::rename(&tmp, &dest)?;
    println!(
        "stockfish: copied {} to {} ({name})",
        source.display(),
        dest.display()
    );
    Ok(())
}

/// Each installed download gets a `<file>.pin` stamp holding the digest it was installed
/// from. The stamp is written last, so a missing or different stamp means the install was
/// interrupted or the pin has changed since, and the file must be installed again.
fn is_installed(dest: &Path, pin: &str) -> bool {
    dest.exists() && std::fs::read_to_string(stamp_path(dest)).is_ok_and(|s| s.trim() == pin)
}

fn mark_installed(dest: &Path, pin: &str) -> Result<()> {
    std::fs::write(stamp_path(dest), format!("{pin}\n"))?;
    Ok(())
}

fn stamp_path(dest: &Path) -> PathBuf {
    let mut path = dest.as_os_str().to_owned();
    path.push(".pin");
    PathBuf::from(path)
}

/// Creates `dir` empty, removing anything left in it by an earlier interrupted run.
fn fresh_dir(dir: &Path) -> Result<PathBuf> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    Ok(dir.to_path_buf())
}

/// Resolves a command name on PATH, or returns the path unchanged if it has a separator.
fn resolve_command(command: &str) -> Result<PathBuf> {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return Ok(PathBuf::from(command));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
        .with_context(|| format!("`{command}` not found on PATH; pass --stockfish <path>"))
}

fn download(url: &str, dest: &Path, expected_sha256: &str) -> Result<()> {
    println!("downloading {url}");
    run_tool(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--retry",
                "3",
            ])
            .arg("--output")
            .arg(dest)
            .arg(url),
    )?;
    let bytes = std::fs::read(dest)?;
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != expected_sha256 {
        std::fs::remove_file(dest)?;
        bail!("SHA-256 mismatch for {url}: expected {expected_sha256}, got {actual}");
    }
    Ok(())
}

fn run_tool(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .with_context(|| format!("failed to run `{program}`"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}
