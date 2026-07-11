//! The Tauri command surface: thin adapters over the mediator.
//! Every command that computes or touches `SQLite` is async and dispatches on a
//! blocking worker via `spawn_blocking(send_now)` — never the webview thread.

pub(crate) mod app_state;

use std::path::PathBuf;

/// The app-state data root (`GIT_TOOLS_DATA_DIR` override honored by the PAL) —
/// the same resolution the daemon uses, so both processes share one `gtl.db`.
pub(crate) fn data_root() -> Result<PathBuf, String> {
    gtl_platform::paths::store_root().map_err(|err| err.to_string())
}
