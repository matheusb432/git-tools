//! Builds snapshot recipes for managed repositories with unpushed commits.

use domain::managed::ManagedRepo;
use gtl_recipe::Recipe;

use super::{RecipeRequest, pin};
use crate::{managed::select_unpushed, ports::GitRunner};

/// Requests recipes for ahead repositories from an already-resolved manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildManagedRecipes {
    pub repos: Vec<ManagedRepo>,
    pub operation: RecipeRequest,
}

/// Reports a failure while selecting managed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildManagedRecipesError {
    /// Managed-repository selection failed.
    #[error(transparent)]
    Select(#[from] select_unpushed::SelectUnpushedError),
    /// Immutable pin resolution failed after repository selection.
    #[error(transparent)]
    Pin(#[from] pin::PinRecipeError),
}

/// Builds one complete recipe per present managed repository ahead of upstream.
///
/// # Errors
///
/// Returns [`BuildManagedRecipesError`] when selection or immutable pin resolution fails.
#[cqrsy::handler(query)]
pub fn execute(
    query: BuildManagedRecipes,
    git: &impl GitRunner,
) -> Result<Vec<Recipe>, BuildManagedRecipesError> {
    let selected =
        select_unpushed::execute(select_unpushed::SelectUnpushed { repos: query.repos }, git)?;
    selected
        .into_iter()
        .map(|repo| pin::build_resolved(repo.path, query.operation.clone(), Some(repo.label), git))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use domain::managed::ManagedRepo;
    use gtl_recipe::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{BuildManagedRecipes, execute};
    use crate::{diffs::DiffTarget, recipes::RecipeRequest, testing::FakeGitRunner};

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: "origin".into(),
        }
    }

    #[test]
    fn managed_build_selects_only_ahead_repositories_and_keeps_manifest_order() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("origin/main\n"),
            FakeGitRunner::ok("2\n"),
            FakeGitRunner::ok("/real/api\n"),
            FakeGitRunner::ok("origin/main\n"),
            FakeGitRunner::ok("0\n"),
            FakeGitRunner::ok("origin/main\n"),
            FakeGitRunner::ok("1\n"),
            FakeGitRunner::ok("/real/web\n"),
            FakeGitRunner::ok("api-base\n"),
            FakeGitRunner::ok("api-head\n"),
            FakeGitRunner::ok("web-base\n"),
            FakeGitRunner::ok("web-head\n"),
        ]);

        let recipes = execute(
            BuildManagedRecipes {
                repos: vec![repo("api"), repo("clean"), repo("web")],
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
            },
            &git,
        )
        .expect("managed recipes build");

        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("api"), Some("web")]
        );
        assert_eq!(
            recipes[0].source,
            RecipeSource::LocalRepo("/real/api".into())
        );
        assert_eq!(
            recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(PinnedRange {
                        base: "api-base".into(),
                        head: "api-head".into(),
                    })
                }
            }
        );
    }

    #[test]
    fn pin_transport_propagates_through_the_managed_operation_error() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok("origin/main\n")),
            Ok(FakeGitRunner::ok("1\n")),
            Ok(FakeGitRunner::ok("/real/api\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(
            BuildManagedRecipes {
                repos: vec![repo("api")],
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
            },
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
