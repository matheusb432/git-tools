//! Shared working-tree reads for managed commit and status operations.

use std::path::Path;

use gtl_models::managed::working_tree::DirtyState;

use crate::ports::GitClient;

pub(super) fn read(git: &impl GitClient, repo_path: &Path) -> anyhow::Result<DirtyState> {
    if !git.repo_present(repo_path) {
        return Ok(DirtyState {
            present: false,
            dirty: false,
            files: Vec::new(),
        });
    }

    let files = match git.working_tree(repo_path)? {
        crate::ports::GitEffect::Applied(tree) => tree.files,
        crate::ports::GitEffect::Rejected(_) => Vec::new(),
    };

    Ok(DirtyState {
        present: true,
        dirty: !files.is_empty(),
        files,
    })
}
