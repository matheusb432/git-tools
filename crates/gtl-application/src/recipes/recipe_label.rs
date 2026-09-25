//! Names recipes with the typed labels that tabs, feedback, and history share.

use gtl_models::{
    git::{CommitCount, GitHead, GitRevision},
    paths::ProjectName,
    recipes::{RecipeLabel, RecipeLabelChanges},
};

use super::{Recipe, RecipeOp, RecipeTarget};
use crate::diffs::View;

/// The facts a computed render adds to its recipe's label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RecipeLabelParts {
    /// The recipe alone describes the render, or its computed facts are unknown.
    #[default]
    None,
    /// The number of unpushed commits the render showed.
    UnpushedCommits { count: CommitCount },
    /// The branch and upstream a merge render compared.
    Merge {
        branch: GitHead,
        upstream: GitRevision,
    },
}

impl RecipeLabelParts {
    /// Takes the facts `recipe`'s label shows from its computed `view`.
    #[must_use]
    pub fn from_view(recipe: &Recipe, view: &View) -> Self {
        match &recipe.op {
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { .. },
            } => Self::UnpushedCommits {
                count: CommitCount::new(view.commits.len() as u64),
            },
            RecipeOp::Diff {
                target: RecipeTarget::Merge { .. },
            }
            | RecipeOp::MergeDiff { .. } => Self::Merge {
                branch: view.branch.clone(),
                upstream: view.upstream.clone(),
            },
            RecipeOp::Diff { .. } => Self::None,
        }
    }
}

/// Names `recipe` before its render is computed.
pub(crate) fn pending(recipe: &Recipe) -> RecipeLabel {
    rendered(recipe, recipe.cwd().project_name(), &RecipeLabelParts::None)
}

/// Names a render of `recipe` in `repository` with the `parts` its computation found.
pub(crate) fn rendered(
    recipe: &Recipe,
    repository: ProjectName,
    parts: &RecipeLabelParts,
) -> RecipeLabel {
    match &recipe.name {
        Some(name) => RecipeLabel::Named { name: name.clone() },
        None => RecipeLabel::Changes {
            repository,
            changes: changes(&recipe.op, parts),
        },
    }
}

fn changes(op: &RecipeOp, parts: &RecipeLabelParts) -> RecipeLabelChanges {
    match (op, parts) {
        (
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { .. },
            },
            RecipeLabelParts::UnpushedCommits { count },
        ) => RecipeLabelChanges::UnpushedCommits { count: *count },
        (
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { .. },
            },
            _,
        ) => RecipeLabelChanges::Unpushed,
        (
            RecipeOp::Diff {
                target: RecipeTarget::Base { rev },
            },
            _,
        ) => RecipeLabelChanges::WorkingTree { base: rev.clone() },
        (
            RecipeOp::Diff {
                target: RecipeTarget::Range { range, .. },
            },
            _,
        ) => RecipeLabelChanges::Range {
            range: range.clone(),
        },
        (
            RecipeOp::Diff {
                target: RecipeTarget::Merge { .. },
            }
            | RecipeOp::MergeDiff { .. },
            RecipeLabelParts::Merge { branch, upstream },
        ) => RecipeLabelChanges::Merge {
            branch: branch.clone(),
            upstream: upstream.clone(),
        },
        (
            RecipeOp::Diff {
                target: RecipeTarget::Merge { base, .. },
            },
            _,
        ) => RecipeLabelChanges::MergeInto { base: base.clone() },
        (RecipeOp::MergeDiff { base, .. }, _) => RecipeLabelChanges::MergeInto {
            base: base.clone().unwrap_or_else(GitRevision::main),
        },
        (
            RecipeOp::Diff {
                target: RecipeTarget::Last { count, .. },
            },
            _,
        ) => RecipeLabelChanges::LastCommits { count: *count },
    }
}

/// Names a live tab by the repository it follows when its range moves with `HEAD`.
pub(crate) fn live(recipe: &Recipe) -> Option<RecipeLabel> {
    match &recipe.op {
        RecipeOp::Diff {
            target: RecipeTarget::Base { rev },
        } if *rev == GitRevision::head() => {}
        RecipeOp::Diff {
            target: RecipeTarget::Unpushed { .. },
        } => {}
        _ => return None,
    }
    Some(recipe.name.clone().map_or_else(
        || RecipeLabel::Repository {
            repository: recipe.cwd().project_name(),
        },
        |name| RecipeLabel::Named { name },
    ))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        recipes::RecipeSource,
        utils::viewer::{empty_view, recipe},
    };

    fn changes_label(changes: RecipeLabelChanges) -> RecipeLabel {
        RecipeLabel::Changes {
            repository: crate::utils::project_name("project"),
            changes,
        }
    }

    fn every_recipe_op() -> [RecipeOp; 7] {
        [
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: crate::utils::git_revision("v1"),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: crate::utils::git_range("v1..v2"),
                    pinned: None,
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: crate::utils::git_revision("release"),
                    pinned: None,
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Last {
                    count: NonZeroU32::new(2).unwrap(),
                    pinned: None,
                },
            },
            RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            RecipeOp::MergeDiff {
                base: Some(crate::utils::git_revision("release")),
                pinned: None,
            },
        ]
    }

    #[test]
    fn pending_labels_describe_the_recipe_before_computation() {
        let expected = [
            RecipeLabelChanges::Unpushed,
            RecipeLabelChanges::WorkingTree {
                base: crate::utils::git_revision("v1"),
            },
            RecipeLabelChanges::Range {
                range: crate::utils::git_range("v1..v2"),
            },
            RecipeLabelChanges::MergeInto {
                base: crate::utils::git_revision("release"),
            },
            RecipeLabelChanges::LastCommits {
                count: NonZeroU32::new(2).unwrap(),
            },
            RecipeLabelChanges::MergeInto {
                base: crate::utils::git_revision("main"),
            },
            RecipeLabelChanges::MergeInto {
                base: crate::utils::git_revision("release"),
            },
        ];

        for (op, expected) in every_recipe_op().into_iter().zip(expected) {
            assert_eq!(pending(&recipe(op)), changes_label(expected));
        }
    }

    #[test]
    fn rendered_labels_add_the_computed_parts_their_recipe_shows() {
        let mut view = empty_view();
        view.commits = vec![crate::utils::diffs::commit("abc1234")];
        let merge = RecipeLabelChanges::Merge {
            branch: view.branch.clone(),
            upstream: view.upstream.clone(),
        };
        let expected = [
            RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(1),
            },
            RecipeLabelChanges::WorkingTree {
                base: crate::utils::git_revision("v1"),
            },
            RecipeLabelChanges::Range {
                range: crate::utils::git_range("v1..v2"),
            },
            merge.clone(),
            RecipeLabelChanges::LastCommits {
                count: NonZeroU32::new(2).unwrap(),
            },
            merge.clone(),
            merge,
        ];

        for (op, expected) in every_recipe_op().into_iter().zip(expected) {
            let recipe = recipe(op);
            let parts = RecipeLabelParts::from_view(&recipe, &view);

            assert_eq!(
                rendered(&recipe, view.repo_name.clone(), &parts),
                changes_label(expected)
            );
        }
    }

    #[test]
    fn explicit_name_overrides_every_label() {
        let mut named = recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        });
        named.name = Some(crate::utils::project_name("Release review"));
        let expected = RecipeLabel::Named {
            name: crate::utils::project_name("Release review"),
        };

        assert_eq!(pending(&named), expected);
        assert_eq!(
            rendered(
                &named,
                crate::utils::project_name("project"),
                &RecipeLabelParts::None
            ),
            expected
        );
        assert_eq!(live(&named), Some(expected));
    }

    #[test]
    fn live_labels_name_the_repository_only_when_the_range_follows_head() {
        let repository = RecipeLabel::Repository {
            repository: crate::utils::project_name("project"),
        };

        assert_eq!(
            live(&recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            })),
            Some(repository.clone())
        );
        assert_eq!(
            live(&recipe(RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: GitRevision::head(),
                },
            })),
            Some(repository)
        );
        assert_eq!(
            live(&recipe(RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: crate::utils::git_revision("v1"),
                },
            })),
            None
        );
    }

    #[test]
    fn root_repository_uses_its_full_path_as_the_label() {
        let mut root = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        root.source = RecipeSource::LocalRepo(crate::utils::repository_root("/"));

        assert_eq!(
            pending(&root),
            RecipeLabel::Changes {
                repository: crate::utils::project_name("repo"),
                changes: RecipeLabelChanges::MergeInto {
                    base: crate::utils::git_revision("main"),
                },
            }
        );
    }
}
