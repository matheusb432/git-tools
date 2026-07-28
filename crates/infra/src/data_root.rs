//! Resolves the shared application data root.
use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;

/// Resolves the application data root without creating it.
///
/// # Examples
///
/// ```no_run
/// let data_root = infra::data_root::resolve()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when neither `GIT_TOOLS_DATA_DIR` nor the operating
/// system's application-data directory can be resolved.
pub fn resolve() -> anyhow::Result<PathBuf> {
    let env_override = std::env::var_os("GIT_TOOLS_DATA_DIR").map(PathBuf::from);
    let project_data =
        ProjectDirs::from("", "", "git-tools").map(|dirs| dirs.data_dir().to_path_buf());
    env_override
        .or(project_data)
        .context("could not resolve a data directory (no GIT_TOOLS_DATA_DIR, no home)")
}
