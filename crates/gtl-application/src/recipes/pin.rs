//! Builds one snapshot recipe and pins its symbolic Git range when possible.

use std::path::PathBuf;

use gtl_wire::recipes::{Recipe, RecipeOp};

use crate::{discovery::resolve_repo_top, ports::GitClient, recipes::build_resolved};

/// Requests one complete snapshot recipe for a repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRecipe {
    pub repo_path: PathBuf,
    pub operation: RecipeOp,
    pub name: Option<String>,
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
        source: resolve_repo_top::ResolveRepoTopError,
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
    let top = resolve_repo_top::execute(
        resolve_repo_top::ResolveRepoTop {
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
    use std::{num::NonZeroU32, path::PathBuf};

    use gtl_wire::recipes::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{PinRecipe, execute};
    use crate::testing::ScriptedGitClient;

    fn pin(
        operation: RecipeOp,
        outputs: Vec<crate::testing::GitResponse>,
    ) -> gtl_wire::recipes::Recipe {
        execute(
            PinRecipe {
                repo_path: "/work/repo/nested".into(),
                operation,
                name: Some("repo".into()),
            },
            &ScriptedGitClient::new(outputs),
        )
        .expect("recipe is built")
    }

    #[test]
    fn exact_range_is_pinned_to_immutable_shas() {
        let recipe = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: "main..HEAD".into(),
                    pinned: None,
                },
            },
            vec![
                ScriptedGitClient::applied("/work/repo\n"),
                ScriptedGitClient::applied("base-sha\n"),
                ScriptedGitClient::applied("head-sha\n"),
            ],
        );

        assert_eq!(
            recipe,
            gtl_wire::recipes::Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/work/repo")),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: "main..HEAD".into(),
                        pinned: Some(PinnedRange {
                            base: "base-sha".into(),
                            head: "head-sha".into(),
                        }),
                    },
                },
                name: Some("repo".into()),
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
                        pinned: Some(PinnedRange {
                            base: "base-sha".into(),
                            head: "head-sha".into(),
                        }),
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
                        pinned: Some(PinnedRange {
                            base: "base-sha".into(),
                            head: "head-sha".into(),
                        }),
                    },
                },
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: "release".into(),
                        pinned: None,
                    },
                },
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: "release".into(),
                        pinned: Some(PinnedRange {
                            base: "base-sha".into(),
                            head: "head-sha".into(),
                        }),
                    },
                },
            ),
            (
                RecipeOp::MergeDiff {
                    base: Some("release".into()),
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: Some("release".into()),
                    pinned: Some(PinnedRange {
                        base: "base-sha".into(),
                        head: "head-sha".into(),
                    }),
                },
            ),
            (
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: Some(PinnedRange {
                        base: "base-sha".into(),
                        head: "head-sha".into(),
                    }),
                },
            ),
        ];

        for (operation, expected) in cases {
            let recipe = pin(
                operation,
                vec![
                    ScriptedGitClient::applied("/work/repo\n"),
                    ScriptedGitClient::applied("base-sha\n"),
                    ScriptedGitClient::applied("head-sha\n"),
                ],
            );
            assert_eq!(recipe.op, expected);
        }
    }

    #[test]
    fn base_and_existing_pins_are_preserved_without_resolution() {
        let pinned = PinnedRange {
            base: "already-base".into(),
            head: "already-head".into(),
        };
        let base = pin(
            RecipeOp::Diff {
                target: RecipeTarget::Base { rev: "main".into() },
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
                target: RecipeTarget::Base { rev: "main".into() }
            }
        );
        assert_eq!(
            existing.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(PinnedRange {
                        base: "already-base".into(),
                        head: "already-head".into(),
                    })
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
                    range: "main...HEAD".into(),
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
                    range: "main...HEAD".into(),
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

        let recipe = execute(
            PinRecipe {
                repo_path: "/work/repo".into(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: Some("repo".into()),
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
