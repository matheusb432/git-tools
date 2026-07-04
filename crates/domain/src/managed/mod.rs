//! The managed-repos feature's domain model: one entry from the fleet manifest.

use std::path::PathBuf;

/// One repo listed in the managed-repos manifest (`repos.toml`), resolved to an
/// absolute local path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
}
