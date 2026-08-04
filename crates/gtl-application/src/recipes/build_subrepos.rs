//! Builds snapshot recipes for every repository discovered under a root.

use std::path::PathBuf;

use gtl_contracts::recipes::Recipe;

use super::RecipeRequest;
use crate::{
    discovery::find_repo_tops,
    ports::{GitClient, RepoDiscovery},
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
}

/// Builds one complete recipe per repository discovered under `query.root`.
///
/// # Errors
///
/// Returns [`BuildSubrepoRecipesError`] when repository discovery or top-level resolution fails.
#[cqrsy::query]
pub fn execute(
    query: BuildSubrepoRecipes,
    discovery: &impl RepoDiscovery,
    git: &impl GitClient,
) -> Result<Vec<Recipe>, BuildSubrepoRecipesError> {
    let repos = find_repo_tops::execute(
        find_repo_tops::FindRepoTops {
            root: query.root,
            include_worktrees: query.include_worktrees,
        },
        discovery,
        git,
    )?;
    Ok(repos
        .into_iter()
        .map(|repo| {
            super::logic::build_resolved(repo.path, query.operation.clone(), Some(repo.label), git)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, path::PathBuf};

    use gtl_contracts::recipes::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{BuildSubrepoRecipes, execute};
    use crate::{
        diffs::DiffTarget, ports::RepoDiscovery, recipes::RecipeRequest, testing::ScriptedGitClient,
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
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("/real/web\n"),
            ScriptedGitClient::applied("api-base\n"),
            ScriptedGitClient::applied("api-head\n"),
            ScriptedGitClient::applied("web-base\n"),
            ScriptedGitClient::applied("web-head\n"),
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
            gtl_contracts::recipes::Recipe {
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
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("/real/web\n"),
            ScriptedGitClient::applied("/real/api-worktree\n"),
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
    fn pin_resolution_failures_keep_subrepo_recipes_symbolic() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("/real/api\n")),
            Ok(ScriptedGitClient::applied("/real/web\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let recipes = execute(
            BuildSubrepoRecipes {
                root: "/scan".into(),
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
                include_worktrees: false,
            },
            &WorktreeAwareDiscovery,
            &git,
        )
        .expect("pin failures are optional optimizations");

        assert!(recipes.iter().all(|recipe| recipe.op
            == RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }));
    }
}
