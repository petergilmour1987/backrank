//! Backrank Chess Engine library.
//!
//! The engine core lives here; `src/main.rs` is a thin binary front end.

pub mod attacks;
pub mod board;
pub mod movegen;
pub mod perft;
mod rng;
pub mod types;
pub mod uci;

pub use board::{FenError, Position};
pub use types::{Bitboard, CastlingRights, Color, File, Move, Piece, PieceType, Rank, Square};

/// Engine name, as reported to GUIs.
pub const ENGINE_NAME: &str = "Backrank";

/// Engine version, taken from `Cargo.toml`.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
