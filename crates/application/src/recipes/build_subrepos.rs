//! Builds snapshot recipes for every repository discovered under a root.

use std::path::PathBuf;

use gtl_recipe::Recipe;

use super::{RecipeRequest, pin};
use crate::{
    discovery::find_repo_tops,
    ports::{GitRunner, RepoDiscovery},
};

/// Requests recipes for every repository discovered under a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSubrepoRecipes {
    pub root: PathBuf,
    pub operation: RecipeRequest,
    pub include_worktrees: bool,
}

/// Reports a failure while discovering recursive repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildSubrepoRecipesError {
    /// Recursive repository discovery or top-level resolution failed.
    #[error(transparent)]
    Discover(#[from] find_repo_tops::FindRepoTopsError),
    /// Immutable pin resolution failed after repository discovery.
    #[error(transparent)]
    Pin(#[from] pin::PinRecipeError),
}

/// Builds one complete recipe per repository discovered under `query.root`.
///
/// # Errors
///
/// Returns [`BuildSubrepoRecipesError`] when discovery, top-level resolution, or pinning fails.
#[cqrsy::query]
pub fn execute(
    query: BuildSubrepoRecipes,
    discovery: &impl RepoDiscovery,
    git: &impl GitRunner,
) -> Result<Vec<Recipe>, BuildSubrepoRecipesError> {
    let repos = find_repo_tops::execute(
        find_repo_tops::FindRepoTops {
            root: query.root,
            include_worktrees: query.include_worktrees,
        },
        discovery,
        git,
    )?;
    repos
        .into_iter()
        .map(|repo| pin::build_resolved(repo.path, query.operation.clone(), Some(repo.label), git))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, num::NonZeroU32, path::PathBuf};

    use gtl_recipe::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{BuildSubrepoRecipes, execute};
    use crate::{
        diffs::DiffTarget, ports::RepoDiscovery, recipes::RecipeRequest, testing::FakeGitRunner,
    };

    #[derive(Clone)]
    struct WorktreeAwareDiscovery;

    impl RepoDiscovery for WorktreeAwareDiscovery {
        fn find_repos(
            &self,
            _root: &std::path::Path,
            include_worktrees: bool,
        ) -> anyhow::Result<Vec<PathBuf>> {
            let mut repos = vec![PathBuf::from("/scan/api"), PathBuf::from("/scan/web")];
            if include_worktrees {
                repos.push(PathBuf::from("/scan/api-worktree"));
            }
            Ok(repos)
        }
    }

    #[test]
    fn recursive_build_preserves_discovery_order_labels_and_last_target() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/real/api\n"),
            FakeGitRunner::ok("/real/web\n"),
            FakeGitRunner::ok("api-base\n"),
            FakeGitRunner::ok("api-head\n"),
            FakeGitRunner::ok("web-base\n"),
            FakeGitRunner::ok("web-head\n"),
        ]);

        let recipes = execute(
            BuildSubrepoRecipes {
                root: "/scan".into(),
                operation: RecipeRequest::Diff(DiffTarget::Last {
                    count: NonZeroU32::new(2).unwrap(),
                    pinned: None,
                }),
                include_worktrees: false,
            },
            &WorktreeAwareDiscovery,
            &git,
        )
        .expect("subrepo recipes build");

        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("api"), Some("web")]
        );
        assert_eq!(
            recipes[0],
            gtl_recipe::Recipe {
                source: RecipeSource::LocalRepo("/real/api".into()),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(2).unwrap(),
                        pinned: Some(PinnedRange {
                            base: "api-base".into(),
                            head: "api-head".into(),
                        }),
                    }
                },
                name: Some("api".into()),
            }
        );
    }

    #[test]
    fn recursive_build_includes_worktrees_when_requested() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/real/api\n"),
            FakeGitRunner::ok("/real/web\n"),
            FakeGitRunner::ok("/real/api-worktree\n"),
        ]);

        let recipes = execute(
            BuildSubrepoRecipes {
                root: "/scan".into(),
                operation: RecipeRequest::Diff(DiffTarget::Base("main".into())),
                include_worktrees: true,
            },
            &WorktreeAwareDiscovery,
            &git,
        )
        .expect("subrepo recipes build");

        assert_eq!(recipes.len(), 3);
        assert_eq!(recipes[2].name.as_deref(), Some("api-worktree"));
    }

    #[test]
    fn pin_transport_propagates_through_the_subrepo_operation_error() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok("/real/api\n")),
            Ok(FakeGitRunner::ok("/real/web\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(
            BuildSubrepoRecipes {
                root: "/scan".into(),
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
                include_worktrees: false,
            },
            &WorktreeAwareDiscovery,
            &git,
        )
        .expect_err("pin transport failure must propagate");

        assert_eq!(
            error.to_string(),
            "git rev-parse @{u}: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
