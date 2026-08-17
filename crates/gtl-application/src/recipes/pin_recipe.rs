//! Builds one snapshot recipe and pins its symbolic Git range when possible.

use std::path::PathBuf;

use gtl_models::paths::ProjectName;
use gtl_wire::recipes::{Recipe, RecipeOp};

use crate::{ports::GitClient, recipes::build_resolved, repositories::resolve_repository_root};

/// Requests one complete snapshot recipe for a repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRecipe {
    pub repo_path: PathBuf,
    pub operation: RecipeOp,
    pub name: Option<ProjectName>,
}

/// Reports a failure to resolve the recipe source repository.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PinRecipeError {
    /// Git could not resolve the requested path to its repository top level.
    #[error("{source}")]
    TopLevel {
        repo_path: PathBuf,
        #[source]
        source: resolve_repository_root::ResolveRepositoryRootError,
    },
}

/// Builds a complete snapshot recipe and pins its symbolic range when possible.
///
/// Pin probes that cannot resolve deliberately leave the operation symbolic.
///
/// # Errors
///
/// Returns [`PinRecipeError::TopLevel`] when Git cannot resolve `query.repo_path` to
/// a repository top level.
#[cqrsy::query]
pub fn execute(query: PinRecipe, git: &impl GitClient) -> Result<Recipe, PinRecipeError> {
    let top = resolve_repository_root::execute(
        resolve_repository_root::ResolveRepositoryRoot {
            repo_path: query.repo_path.clone(),
        },
        git,
    )
    .map_err(|source| PinRecipeError::TopLevel {
        repo_path: query.repo_path.clone(),
        source,
    })?;

    Ok(build_resolved(top, query.operation, query.name, git))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_wire::recipes::{RecipeOp, RecipeSource, RecipeTarget};

    use super::PinRecipe;
    use crate::{recipes::pin_recipe, utils::ScriptedGitClient};

    const BASE_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HEAD_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const EXISTING_BASE_ID: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const EXISTING_HEAD_ID: &str = "dddddddddddddddddddddddddddddddddddddddd";

    fn pin(
        operation: RecipeOp,
        outputs: Vec<crate::utils::GitResponse>,
    ) -> gtl_wire::recipes::Recipe {
        pin_recipe::execute(
            PinRecipe {
                repo_path: "/work/repo/nested".into(),
                operation,
                name: Some(crate::utils::project_name("repo")),
            },
            &ScriptedGitClient::new(outputs),
        )
        .expect("recipe is built")
    }

    #[test]
    fn exact_range_is_pinned_to_immutable_commit_ids() {
        let recipe = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: crate::utils::git_range("main..HEAD"),
                    pinned: None,
                },
            },
            vec![
                ScriptedGitClient::applied("/work/repo\n"),
                ScriptedGitClient::applied(BASE_ID),
                ScriptedGitClient::applied(HEAD_ID),
            ],
        );

        assert_eq!(
            recipe,
            gtl_wire::recipes::Recipe {
                source: RecipeSource::LocalRepo(crate::utils::repository_root("/work/repo")),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: crate::utils::git_range("main..HEAD"),
                        pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                    },
                },
                name: Some(crate::utils::project_name("repo")),
            }
        );
    }

    #[test]
    fn every_pinnable_operation_preserves_its_symbolic_identity() {
        let cases = [
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed {
                        pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                    },
                },
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(3).unwrap(),
                        pinned: None,
                    },
                },
                RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(3).unwrap(),
                        pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                    },
                },
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: crate::utils::git_revision("release"),
                        pinned: None,
                    },
                },
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: crate::utils::git_revision("release"),
                        pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                    },
                },
            ),
            (
                RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("release")),
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("release")),
                    pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                },
            ),
            (
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: Some(crate::utils::pinned_range(BASE_ID, HEAD_ID)),
                },
            ),
        ];

        for (operation, expected) in cases {
            let recipe = pin(
                operation,
                vec![
                    ScriptedGitClient::applied("/work/repo\n"),
                    ScriptedGitClient::applied(BASE_ID),
                    ScriptedGitClient::applied(HEAD_ID),
                ],
            );
            assert_eq!(recipe.op, expected);
        }
    }

    #[test]
    fn base_and_existing_pins_are_preserved_without_resolution() {
        let pinned = crate::utils::pinned_range(EXISTING_BASE_ID, EXISTING_HEAD_ID);
        let base = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: crate::utils::git_revision("main"),
                },
            },
            vec![ScriptedGitClient::applied("/work/repo\n")],
        );
        let existing = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(pinned),
                },
            },
            vec![ScriptedGitClient::applied("/work/repo\n")],
        );

        assert_eq!(
            base.op,
            RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: crate::utils::git_revision("main")
                }
            }
        );
        assert_eq!(
            existing.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::utils::pinned_range(
                        EXISTING_BASE_ID,
                        EXISTING_HEAD_ID,
                    ))
                }
            }
        );
    }

    #[test]
    fn failed_or_unsupported_resolution_falls_back_to_symbolic_values() {
        let unresolved = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            vec![
                ScriptedGitClient::applied("/work/repo\n"),
                ScriptedGitClient::rejected("no upstream"),
            ],
        );
        let three_dot = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: crate::utils::git_range("main...HEAD"),
                    pinned: None,
                },
            },
            vec![ScriptedGitClient::applied("/work/repo\n")],
        );

        assert_eq!(
            unresolved.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
        assert_eq!(
            three_dot.op,
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: crate::utils::git_range("main...HEAD"),
                    pinned: None,
                }
            }
        );
    }

    #[test]
    fn pin_resolution_failure_leaves_the_recipe_symbolic() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("/work/repo\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let recipe = pin_recipe::execute(
            PinRecipe {
                repo_path: "/work/repo".into(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: Some(crate::utils::project_name("repo")),
            },
            &git,
        )
        .expect("pin failure is an optional optimization");

        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }
}
