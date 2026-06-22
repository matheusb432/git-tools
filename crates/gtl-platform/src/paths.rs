//! Where the store lives. Pure resolution is `data_dir_from`; `store_root`
//! reads the environment. The XDG/`%LOCALAPPDATA%` mapping is delegated to the
//! `directories` crate, so no `#[cfg]` is needed here.
use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;

/// Resolve the store data dir from explicit inputs, in precedence order:
/// `GIT_TOOLS_DATA_DIR` override, else the platform project-data dir. Pure.
pub fn data_dir_from(
    env_override: Option<PathBuf>,
    project_data: Option<PathBuf>,
) -> Option<PathBuf> {
    env_override.or(project_data)
}

/// The store root for this machine, creating nothing. Errors only when no
/// home/data location can be resolved at all.
pub fn store_root() -> anyhow::Result<PathBuf> {
    let env_override = std::env::var_os("GIT_TOOLS_DATA_DIR").map(PathBuf::from);
    let project_data =
        ProjectDirs::from("", "", "git-tools").map(|dirs| dirs.data_dir().to_path_buf());
    data_dir_from(env_override, project_data)
        .context("could not resolve a data directory (no GIT_TOOLS_DATA_DIR, no home)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins_over_project_data() {
        let got = data_dir_from(
            Some(PathBuf::from("/explicit")),
            Some(PathBuf::from("/home/u/.local/share/git-tools")),
        );
        assert_eq!(got, Some(PathBuf::from("/explicit")));
    }

    #[test]
    fn falls_back_to_project_data() {
        let got = data_dir_from(None, Some(PathBuf::from("/home/u/.local/share/git-tools")));
        assert_eq!(got, Some(PathBuf::from("/home/u/.local/share/git-tools")));
    }

    #[test]
    fn none_when_nothing_resolvable() {
        assert_eq!(data_dir_from(None, None), None);
    }
}
