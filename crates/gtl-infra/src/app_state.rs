//! Process-owned `SQLite` app state and its connection initialization policy.

pub mod diagnostics;

mod db;
pub mod snapshot;
mod state;

pub use state::SqliteAppState;
