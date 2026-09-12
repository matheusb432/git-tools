//! Working-tree reads shared by push preparation and status.

use gtl_models::{paths::RepositoryRoot, repository::working_tree::DirtyState};

use crate::ports::GitClient;

pub(crate) fn read(git: &impl GitClient, repo_path: &RepositoryRoot) -> anyhow::Result<DirtyState> {
    if !git.repo_present(repo_path) {
        return Ok(DirtyState::Absent);
    }

    Ok(match git.working_tree(repo_path)? {
        crate::ports::GitEffect::Applied(tree) => DirtyState::from_files(tree.files),
        crate::ports::GitEffect::Rejected(detail) => DirtyState::Unavailable { detail },
    })
}
