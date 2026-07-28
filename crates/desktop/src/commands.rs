use std::path::PathBuf;

/// The app-state data root shared with the daemon.
pub(crate) fn data_root() -> Result<PathBuf, String> {
    infra::data_root::resolve().map_err(|error| error.to_string())
}
