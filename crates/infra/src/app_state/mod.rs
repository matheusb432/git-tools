//! The app-state store: `SQLite` persistence for live views and the recent-render
//! log (schema + connection policy in `db`, adapter in `store`).

mod db;
mod store;

pub use store::SqliteAppState;
