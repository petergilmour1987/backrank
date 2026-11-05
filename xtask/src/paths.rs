//! Well-known locations in the repository. Everything under `tools/bin`, `tools/books`,
//! `tools/engines`, `results/pgn` and `results/logs` is gitignored.

use std::path::{Path, PathBuf};

/// The workspace root (the parent of the `xtask` crate).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}

pub fn tools_bin() -> PathBuf {
    root().join("tools/bin")
}

pub fn books() -> PathBuf {
    root().join("tools/books")
}

pub fn engines() -> PathBuf {
    root().join("tools/engines")
}

pub fn results_pgn() -> PathBuf {
    root().join("results/pgn")
}

pub fn results_logs() -> PathBuf {
    root().join("results/logs")
}

pub fn fastchess() -> PathBuf {
    tools_bin().join("fastchess")
}

/// The default opening book used for all matches.
pub fn book() -> PathBuf {
    books().join("noob_3moves.epd")
}

/// The pinned Stockfish snapshot, copied by `cargo xtask setup`.
pub fn stockfish() -> PathBuf {
    engines().join("stockfish")
}
