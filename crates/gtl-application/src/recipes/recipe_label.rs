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
    rendered(recipe, recipe.cwd().project_name(), RecipeLabelParts::None)
}

/// Names a render of `recipe` in `repository` with the `parts` its computation found.
pub(crate) fn rendered(
    recipe: &Recipe,
    repository: ProjectName,
    parts: RecipeLabelParts,
) -> RecipeLabel {
    match &recipe.name {
        Some(name) => RecipeLabel::Named { name: name.clone() },
        None => RecipeLabel::Changes {
            repository,
            changes: changes(&recipe.op, parts),
        },
    }
}

/// Names a live tab by the repository it follows when its range moves with `HEAD`.
pub(crate) fn live(recipe: &Recipe) -> Option<RecipeLabel> {
    if !follows_head(&recipe.op) {
        return None;
    }
    Some(match &recipe.name {
        Some(name) => RecipeLabel::Named { name: name.clone() },
        None => RecipeLabel::Repository {
            repository: recipe.cwd().project_name(),
        },
    })
}

fn follows_head(op: &RecipeOp) -> bool {
    match op {
        RecipeOp::Diff {
            target: RecipeTarget::Base { rev },
        } => *rev == GitRevision::head(),
        RecipeOp::Diff {
            target: RecipeTarget::Unpushed { .. },
        } => true,
        RecipeOp::Diff { .. } | RecipeOp::MergeDiff { .. } => false,
    }
}

/// Describes the changes `op` compares, refined by the computed `parts` that match it.
fn changes(op: &RecipeOp, parts: RecipeLabelParts) -> RecipeLabelChanges {
    match op {
        RecipeOp::Diff { target } => diff_changes(target, parts),
        RecipeOp::MergeDiff {
            base: Some(base), ..
        } => merge_changes(base, parts),
        RecipeOp::MergeDiff { base: None, .. } => merge_changes(&GitRevision::main(), parts),
    }
}

fn diff_changes(target: &RecipeTarget, parts: RecipeLabelParts) -> RecipeLabelChanges {
    match target {
        RecipeTarget::Unpushed { .. } => match parts {
            RecipeLabelParts::UnpushedCommits { count } => {
                RecipeLabelChanges::UnpushedCommits { count }
            }
            RecipeLabelParts::None | RecipeLabelParts::Merge { .. } => RecipeLabelChanges::Unpushed,
        },
        RecipeTarget::Base { rev } => RecipeLabelChanges::WorkingTree { base: rev.clone() },
        RecipeTarget::Range { range, .. } => RecipeLabelChanges::Range {
            range: range.clone(),
        },
        RecipeTarget::Merge { base, .. } => merge_changes(base, parts),
        RecipeTarget::Last { count, .. } => RecipeLabelChanges::LastCommits { count: *count },
    }
}

fn merge_changes(base: &GitRevision, parts: RecipeLabelParts) -> RecipeLabelChanges {
    match parts {
        RecipeLabelParts::Merge { branch, upstream } => {
            RecipeLabelChanges::Merge { branch, upstream }
        }
        RecipeLabelParts::None | RecipeLabelParts::UnpushedCommits { .. } => {
            RecipeLabelChanges::MergeInto { base: base.clone() }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        recipes::RecipeSource,
        utils::{
            diffs::commit,
            git_range, git_revision, project_name, repository_root,
            viewer::{empty_view, recipe},
        },
    };

    fn changes_label(changes: RecipeLabelChanges) -> RecipeLabel {
        RecipeLabel::Changes {
            repository: project_name("project"),
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
                    rev: git_revision("v1"),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: git_range("v1..v2"),
                    pinned: None,
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: git_revision("release"),
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
                base: Some(git_revision("release")),
                pinned: None,
            },
        ]
    }

    #[test]
    fn pending_labels_describe_the_recipe_before_computation() {
        let expected = [
            RecipeLabelChanges::Unpushed,
            RecipeLabelChanges::WorkingTree {
                base: git_revision("v1"),
            },
            RecipeLabelChanges::Range {
                range: git_range("v1..v2"),
            },
            RecipeLabelChanges::MergeInto {
                base: git_revision("release"),
            },
            RecipeLabelChanges::LastCommits {
                count: NonZeroU32::new(2).unwrap(),
            },
            RecipeLabelChanges::MergeInto {
                base: git_revision("main"),
            },
            RecipeLabelChanges::MergeInto {
                base: git_revision("release"),
            },
        ];

        for (op, expected) in every_recipe_op().into_iter().zip(expected) {
            assert_eq!(pending(&recipe(op)), changes_label(expected));
        }
    }

    #[test]
    fn rendered_labels_add_the_computed_parts_their_recipe_shows() {
        let mut view = empty_view();
        view.commits = vec![commit("abc1234")];
        let merge = RecipeLabelChanges::Merge {
            branch: view.branch.clone(),
            upstream: view.upstream.clone(),
        };
        let expected = [
            RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(1),
            },
            RecipeLabelChanges::WorkingTree {
                base: git_revision("v1"),
            },
            RecipeLabelChanges::Range {
                range: git_range("v1..v2"),
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
                rendered(&recipe, view.repo_name.clone(), parts),
                changes_label(expected)
            );
        }
    }

    #[test]
    fn explicit_name_overrides_every_label() {
        let mut named = recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        });
        named.name = Some(project_name("Release review"));
        let expected = RecipeLabel::Named {
            name: project_name("Release review"),
        };

        assert_eq!(pending(&named), expected);
        assert_eq!(
            rendered(&named, project_name("project"), RecipeLabelParts::None),
            expected
        );
        assert_eq!(live(&named), Some(expected));
    }

    #[test]
    fn live_labels_name_the_repository_only_when_the_range_follows_head() {
        let repository = RecipeLabel::Repository {
            repository: project_name("project"),
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
                    rev: git_revision("v1"),
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
        root.source = RecipeSource::LocalRepo(repository_root("/"));

        assert_eq!(
            pending(&root),
            RecipeLabel::Changes {
                repository: project_name("repo"),
                changes: RecipeLabelChanges::MergeInto {
                    base: git_revision("main"),
                },
            }
        );
    }
}
