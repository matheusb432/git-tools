use gtl_models::{
    failure::{ErrorMeta, Failure},
    git::{GitHead, GitRevision},
    recipes::RecipeBatchId,
};
use gtl_wire::viewer::commit_search::{OpenViewerCommit, ViewerCommitSearchScope};

use super::{
    ViewerState,
    work::{self, ReserveRecipeError, ReservedRecipeWork},
};
use crate::{
    ports::{GitClient, UserSettingsReader},
    recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum OpenViewerCommitError {
    #[error("commit is no longer on the active branch")]
    #[meta(failure = Failure::Changed)]
    Changed,
    #[error(transparent)]
    #[meta(transparent)]
    Source(#[from] super::source::ViewerSourceError),
    #[error(transparent)]
    #[meta(transparent)]
    Reserve(#[from] ReserveRecipeError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: OpenViewerCommit,
    state: &ViewerState,
    git: &impl GitClient,
    settings: &impl UserSettingsReader,
) -> Result<ReservedRecipeWork, OpenViewerCommitError> {
    let revision = GitRevision::from(&request.id);
    let path = match request.scope {
        ViewerCommitSearchScope::ActiveBranch(path) => path,
        ViewerCommitSearchScope::ActiveBranchSnapshot(identity) => {
            super::search_viewer_commits::commit_source(identity, state, settings)?
                .repo_root
                .clone()
        }
        ViewerCommitSearchScope::Snapshot(identity) => {
            let snapshot = super::search_viewer_commits::commit_source(identity, state, settings)?;
            if !snapshot
                .commits
                .iter()
                .any(|commit| commit.id == request.id)
            {
                return Err(OpenViewerCommitError::Changed);
            }
            return Ok(work::reserve_open(
                state,
                Recipe {
                    source: RecipeSource::LocalRepo(snapshot.repo_root.clone()),
                    op: RecipeOp::Diff {
                        target: RecipeTarget::Commit { rev: revision },
                    },
                    name: None,
                },
                RecipeBatchId::generate(),
            )?);
        }
    };
    if !matches!(git.current_branch(&path)?, GitHead::Branch(_))
        || git.find_merge_base(&path, &GitRevision::head(), &revision)? != Some(request.id)
    {
        return Err(OpenViewerCommitError::Changed);
    }
    Ok(work::reserve_open(
        state,
        Recipe {
            source: RecipeSource::LocalRepo(path),
            op: RecipeOp::Diff {
                target: RecipeTarget::Commit { rev: revision },
            },
            name: None,
        },
        RecipeBatchId::generate(),
    )?)
}
