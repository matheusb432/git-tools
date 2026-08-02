//! Builds one snapshot recipe and pins its symbolic Git range when possible.

use std::path::{Path, PathBuf};

use gtl_contracts::recipes::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};

use super::RecipeRequest;
use crate::{
    diffs::{self, DiffTarget},
    discovery::resolve_repo_top,
    ports::GitClient,
};

/// Requests one complete snapshot recipe for a repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRecipe {
    pub repo: PathBuf,
    pub operation: RecipeRequest,
    pub name: Option<String>,
}

pub type PinRecipeOk = Recipe;

/// Reports a failure to resolve the recipe source repository.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PinRecipeError {
    /// Git could not resolve the requested path to its repository top level.
    #[error("{source}")]
    TopLevel {
        repo: PathBuf,
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
/// Returns [`PinRecipeError::TopLevel`] when Git cannot resolve `query.repo` to
/// a repository top level.
#[cqrsy::query]
pub fn execute(query: PinRecipe, git: &impl GitClient) -> Result<PinRecipeOk, PinRecipeError> {
    let top = resolve_repo_top::execute(
        resolve_repo_top::ResolveRepoTop {
            repo: query.repo.clone(),
        },
        git,
    )
    .map_err(|source| PinRecipeError::TopLevel {
        repo: query.repo.clone(),
        source,
    })?;

    Ok(build_resolved(top, query.operation, query.name, git))
}

pub(super) fn build_resolved(
    repo_top: PathBuf,
    operation: RecipeRequest,
    name: Option<String>,
    git: &impl GitClient,
) -> Recipe {
    Recipe {
        op: pin_operation(&repo_top, operation_to_op(operation), git),
        source: RecipeSource::LocalRepo(repo_top),
        name,
    }
}

fn operation_to_op(operation: RecipeRequest) -> RecipeOp {
    match operation {
        RecipeRequest::Diff(target) => RecipeOp::Diff {
            target: target_to_recipe(target),
        },
        RecipeRequest::MergeDiff { base } => RecipeOp::MergeDiff { base, pinned: None },
        RecipeRequest::SquashPreview => RecipeOp::SquashPreview { pinned: None },
    }
}

fn target_to_recipe(target: DiffTarget) -> RecipeTarget {
    match target {
        DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed {
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Base(rev) => RecipeTarget::Base { rev },
        DiffTarget::Range { range, pinned } => RecipeTarget::Range {
            range,
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Merge { base, pinned } => RecipeTarget::Merge {
            base,
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Last { count, pinned } => RecipeTarget::Last {
            count,
            pinned: pinned.map(pin_to_recipe),
        },
    }
}

fn pin_to_recipe(pin: diffs::PinnedRange) -> PinnedRange {
    PinnedRange {
        base: pin.base,
        head: pin.head,
    }
}

fn pin_operation(repo_top: &Path, operation: RecipeOp, git: &impl GitClient) -> RecipeOp {
    match operation {
        RecipeOp::Diff { target } => RecipeOp::Diff {
            target: pin_target(repo_top, target, git),
        },
        RecipeOp::MergeDiff { base, pinned: None } => RecipeOp::MergeDiff {
            pinned: pin_merge(repo_top, base.as_deref(), git),
            base,
        },
        RecipeOp::SquashPreview { pinned: None } => RecipeOp::SquashPreview {
            pinned: pin_range(repo_top, "@{u}", "HEAD", git),
        },
        operation => operation,
    }
}

fn pin_target(repo_top: &Path, target: RecipeTarget, git: &impl GitClient) -> RecipeTarget {
    match target {
        RecipeTarget::Unpushed { pinned: None } => RecipeTarget::Unpushed {
            pinned: pin_range(repo_top, "@{u}", "HEAD", git),
        },
        RecipeTarget::Last {
            count,
            pinned: None,
        } => RecipeTarget::Last {
            count,
            pinned: pin_range(repo_top, &format!("HEAD~{count}"), "HEAD", git),
        },
        RecipeTarget::Range {
            range,
            pinned: None,
        } => RecipeTarget::Range {
            pinned: pin_exact_range(repo_top, &range, git),
            range,
        },
        RecipeTarget::Merge { base, pinned: None } => RecipeTarget::Merge {
            pinned: pin_merge(repo_top, Some(&base), git),
            base,
        },
        target => target,
    }
}

fn pin_range(repo_top: &Path, base: &str, head: &str, git: &impl GitClient) -> Option<PinnedRange> {
    let base = capture_pin(git.resolve_sha(repo_top, base))?;
    let head = capture_pin(git.resolve_sha(repo_top, head))?;
    Some(PinnedRange { base, head })
}

fn pin_exact_range(repo_top: &Path, range: &str, git: &impl GitClient) -> Option<PinnedRange> {
    if range.contains("...") {
        return None;
    }
    let (base, head) = range.split_once("..")?;
    if base.is_empty() || head.is_empty() {
        return None;
    }
    pin_range(repo_top, base, head, git)
}

fn pin_merge(repo_top: &Path, base: Option<&str>, git: &impl GitClient) -> Option<PinnedRange> {
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(crate::diffs::render_merge_diff::DEFAULT_BASE);
    let base = capture_pin(git.merge_base(repo_top, base, "HEAD"))?;
    let head = capture_pin(git.resolve_sha(repo_top, "HEAD"))?;
    Some(PinnedRange { base, head })
}

fn capture_pin(result: anyhow::Result<String>) -> Option<String> {
    result.ok()
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, path::PathBuf};

    use gtl_contracts::recipes::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{PinRecipe, execute};
    use crate::{
        diffs::{self, DiffTarget},
        recipes::RecipeRequest,
        testing::ScriptedGitClient,
    };

    fn pin(
        operation: RecipeRequest,
        outputs: Vec<crate::testing::GitResponse>,
    ) -> gtl_contracts::recipes::Recipe {
        execute(
            PinRecipe {
                repo: "/work/repo/nested".into(),
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
            RecipeRequest::Diff(DiffTarget::Range {
                range: "main..HEAD".into(),
                pinned: None,
            }),
            vec![
                ScriptedGitClient::applied("/work/repo\n"),
                ScriptedGitClient::applied("base-sha\n"),
                ScriptedGitClient::applied("head-sha\n"),
            ],
        );

        assert_eq!(
            recipe,
            gtl_contracts::recipes::Recipe {
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
                RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
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
                RecipeRequest::Diff(DiffTarget::Last {
                    count: NonZeroU32::new(3).unwrap(),
                    pinned: None,
                }),
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
                RecipeRequest::Diff(DiffTarget::Merge {
                    base: "release".into(),
                    pinned: None,
                }),
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
                RecipeRequest::MergeDiff {
                    base: Some("release".into()),
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
                RecipeRequest::MergeDiff { base: None },
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: Some(PinnedRange {
                        base: "base-sha".into(),
                        head: "head-sha".into(),
                    }),
                },
            ),
            (
                RecipeRequest::SquashPreview,
                RecipeOp::SquashPreview {
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
        let pinned = diffs::PinnedRange {
            base: "already-base".into(),
            head: "already-head".into(),
        };
        let base = pin(
            RecipeRequest::Diff(DiffTarget::Base("main".into())),
            vec![ScriptedGitClient::applied("/work/repo\n")],
        );
        let existing = pin(
            RecipeRequest::Diff(DiffTarget::Unpushed {
                pinned: Some(pinned),
            }),
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
            RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
            vec![
                ScriptedGitClient::applied("/work/repo\n"),
                ScriptedGitClient::rejected("no upstream"),
            ],
        );
        let three_dot = pin(
            RecipeRequest::Diff(DiffTarget::Range {
                range: "main...HEAD".into(),
                pinned: None,
            }),
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
                repo: "/work/repo".into(),
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
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
