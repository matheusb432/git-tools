//! Discovered-repo data shape shared by the discovery feature slice and its callers.

use std::path::PathBuf;

/// A git repo found under a discovery root: its directory and a display label
/// relative to that root. A dumb data carrier — the discovery query builds it and
/// callers read it; no behavior lives here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepo {
    /// The discovered repo directory (the dir that contains `.git`).
    pub path: PathBuf,
    /// Display label relative to the discovery root (e.g. `libs/inner`).
    pub label: String,
}
