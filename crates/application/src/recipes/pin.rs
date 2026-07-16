//! Builds one snapshot recipe and pins its symbolic Git range when possible.

use std::path::{Path, PathBuf};

use gtl_recipe::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};

use super::RecipeRequest;
use crate::{
    diffs::{DiffTarget, PinnedRange as DiffPinnedRange},
    discovery::resolve_repo_top,
    ports::GitRunner,
    shared::git::{capture_checked, command_label},
};

/// Requests one complete snapshot recipe for a repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRecipe {
    pub repo: PathBuf,
    pub operation: RecipeRequest,
    pub name: Option<String>,
}

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
    /// Git transport failed while resolving an immutable pin.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a complete snapshot recipe and pins its symbolic range when possible.
///
/// Pin probes that Git rejects with a nonzero exit deliberately leave the
/// operation symbolic. Git transport failures reject construction.
///
/// # Errors
///
/// Returns [`PinRecipeError::TopLevel`] when Git cannot resolve `query.repo` to
/// a repository top level. Returns [`PinRecipeError::Transport`] when Git
/// transport fails while resolving an immutable pin.
#[cqrsy::handler(query)]
pub fn execute(query: PinRecipe, git: &impl GitRunner) -> Result<Recipe, PinRecipeError> {
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

    build_resolved(top, query.operation, query.name, git)
}

pub(super) fn build_resolved(
    repo_top: PathBuf,
    operation: RecipeRequest,
    name: Option<String>,
    git: &impl GitRunner,
) -> Result<Recipe, PinRecipeError> {
    Ok(Recipe {
        op: pin_operation(&repo_top, operation_to_op(operation), git)?,
        source: RecipeSource::LocalRepo(repo_top),
        name,
    })
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

fn pin_to_recipe(pin: DiffPinnedRange) -> PinnedRange {
    PinnedRange {
        base: pin.base,
        head: pin.head,
    }
}

fn pin_operation(
    repo_top: &Path,
    operation: RecipeOp,
    git: &impl GitRunner,
) -> Result<RecipeOp, PinRecipeError> {
    Ok(match operation {
        RecipeOp::Diff { target } => RecipeOp::Diff {
            target: pin_target(repo_top, target, git)?,
        },
        RecipeOp::MergeDiff { base, pinned: None } => RecipeOp::MergeDiff {
            pinned: pin_merge(repo_top, base.as_deref(), git)?,
            base,
        },
        RecipeOp::SquashPreview { pinned: None } => RecipeOp::SquashPreview {
            pinned: pin_range(repo_top, "@{u}", "HEAD", git)?,
        },
        operation => operation,
    })
}

fn pin_target(
    repo_top: &Path,
    target: RecipeTarget,
    git: &impl GitRunner,
) -> Result<RecipeTarget, PinRecipeError> {
    Ok(match target {
        RecipeTarget::Unpushed { pinned: None } => RecipeTarget::Unpushed {
            pinned: pin_range(repo_top, "@{u}", "HEAD", git)?,
        },
        RecipeTarget::Last {
            count,
            pinned: None,
        } => RecipeTarget::Last {
            count,
            pinned: pin_range(repo_top, &format!("HEAD~{count}"), "HEAD", git)?,
        },
        RecipeTarget::Range {
            range,
            pinned: None,
        } => RecipeTarget::Range {
            pinned: pin_exact_range(repo_top, &range, git)?,
            range,
        },
        RecipeTarget::Merge { base, pinned: None } => RecipeTarget::Merge {
            pinned: pin_merge(repo_top, Some(&base), git)?,
            base,
        },
        target => target,
    })
}

fn pin_range(
    repo_top: &Path,
    base: &str,
    head: &str,
    git: &impl GitRunner,
) -> Result<Option<PinnedRange>, PinRecipeError> {
    let base_args = ["rev-parse", base];
    let Some(base) = capture_pin(git, repo_top, &base_args)? else {
        return Ok(None);
    };
    let head_args = ["rev-parse", head];
    let Some(head) = capture_pin(git, repo_top, &head_args)? else {
        return Ok(None);
    };
    Ok(Some(PinnedRange { base, head }))
}

fn pin_exact_range(
    repo_top: &Path,
    range: &str,
    git: &impl GitRunner,
) -> Result<Option<PinnedRange>, PinRecipeError> {
    if range.contains("...") {
        return Ok(None);
    }
    let Some((base, head)) = range.split_once("..") else {
        return Ok(None);
    };
    if base.is_empty() || head.is_empty() {
        return Ok(None);
    }
    pin_range(repo_top, base, head, git)
}

fn pin_merge(
    repo_top: &Path,
    base: Option<&str>,
    git: &impl GitRunner,
) -> Result<Option<PinnedRange>, PinRecipeError> {
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(crate::diffs::render_merge_diff::DEFAULT_BASE);
    let base_args = ["merge-base", base, "HEAD"];
    let Some(base) = capture_pin(git, repo_top, &base_args)? else {
        return Ok(None);
    };
    let head_args = ["rev-parse", "HEAD"];
    let Some(head) = capture_pin(git, repo_top, &head_args)? else {
        return Ok(None);
    };
    Ok(Some(PinnedRange { base, head }))
}

fn capture_pin(
    git: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> Result<Option<String>, PinRecipeError> {
    capture_checked(git, repo, args).map_err(|source| PinRecipeError::Transport {
        command: command_label(args),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, num::NonZeroU32, path::PathBuf};

    use gtl_recipe::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::{PinRecipe, execute};
    use crate::{
        diffs::{DiffTarget, PinnedRange as DiffPinnedRange},
        recipes::RecipeRequest,
        testing::FakeGitRunner,
    };

    fn pin(operation: RecipeRequest, outputs: Vec<crate::ports::GitOutput>) -> gtl_recipe::Recipe {
        execute(
            PinRecipe {
                repo: "/work/repo/nested".into(),
                operation,
                name: Some("repo".into()),
            },
            &FakeGitRunner::new(outputs),
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
                FakeGitRunner::ok("/work/repo\n"),
                FakeGitRunner::ok("base-sha\n"),
                FakeGitRunner::ok("head-sha\n"),
            ],
        );

        assert_eq!(
            recipe,
            gtl_recipe::Recipe {
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
                    FakeGitRunner::ok("/work/repo\n"),
                    FakeGitRunner::ok("base-sha\n"),
                    FakeGitRunner::ok("head-sha\n"),
                ],
            );
            assert_eq!(recipe.op, expected);
        }
    }

    #[test]
    fn base_and_existing_pins_are_preserved_without_resolution() {
        let pinned = DiffPinnedRange {
            base: "already-base".into(),
            head: "already-head".into(),
        };
        let base = pin(
            RecipeRequest::Diff(DiffTarget::Base("main".into())),
            vec![FakeGitRunner::ok("/work/repo\n")],
        );
        let existing = pin(
            RecipeRequest::Diff(DiffTarget::Unpushed {
                pinned: Some(pinned),
            }),
            vec![FakeGitRunner::ok("/work/repo\n")],
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
                FakeGitRunner::ok("/work/repo\n"),
                FakeGitRunner::exit_err("no upstream", 128),
            ],
        );
        let three_dot = pin(
            RecipeRequest::Diff(DiffTarget::Range {
                range: "main...HEAD".into(),
                pinned: None,
            }),
            vec![FakeGitRunner::ok("/work/repo\n")],
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
    fn pin_transport_failure_remains_a_sourced_error() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok("/work/repo\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(
            PinRecipe {
                repo: "/work/repo".into(),
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
                name: Some("repo".into()),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

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
