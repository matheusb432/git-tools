//! Process-owned `SQLite` app state and its connection initialization policy.

mod db;
pub mod snapshot;
mod state;

pub use state::SqliteAppState;
