//! Names recipes with the typed labels that tabs, feedback, and history share.

use gtl_models::{
    diffs::CommitId,
    git::{CommitCount, GitHead, GitRange, GitRevision},
    paths::ProjectName,
    recipes::{RecipeLabel, RecipeLabelChanges, RecipeLabelHead},
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
        match recipe.op() {
            Some(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { .. },
            }) => Self::UnpushedCommits {
                count: CommitCount::new(view.commits.len() as u64),
            },
            Some(
                RecipeOp::Diff {
                    target: RecipeTarget::Merge { .. },
                }
                | RecipeOp::MergeDiff { .. },
            ) => view
                .origin
                .repository()
                .map_or(Self::None, |repository| Self::Merge {
                    branch: repository.branch.clone(),
                    upstream: repository.upstream.clone(),
                }),
            Some(RecipeOp::Diff { .. }) | None => Self::None,
        }
    }
}

/// Names `recipe` before its render is computed.
pub(crate) fn pending(recipe: &Recipe) -> RecipeLabel {
    rendered(recipe, recipe.source_name(), RecipeLabelParts::None)
}

/// Names a render of `recipe` in `repository` with the `parts` its computation found.
pub(crate) fn rendered(
    recipe: &Recipe,
    repository: ProjectName,
    parts: RecipeLabelParts,
) -> RecipeLabel {
    match (&recipe.name, recipe.op()) {
        (Some(name), _) => RecipeLabel::Named { name: name.clone() },
        (None, Some(op)) => RecipeLabel::Changes {
            repository,
            changes: changes(op, parts),
        },
        (None, None) => RecipeLabel::Repository { repository },
    }
}

/// Names a tab before its render resolves the revisions it compares.
pub(crate) fn pending_tab(recipe: &Recipe) -> RecipeLabel {
    match &recipe.name {
        Some(name) => RecipeLabel::Named { name: name.clone() },
        None => RecipeLabel::Repository {
            repository: recipe.source_name(),
        },
    }
}

/// Names a tab by the revisions its render of `recipe` compared, preferring names to commit IDs.
///
/// `comparison` names the upstream or comparison branch an unpushed recipe compared against.
pub(crate) fn compared(
    recipe: &Recipe,
    view: &View,
    comparison: Option<&GitRevision>,
) -> RecipeLabel {
    if let Some(name) = &recipe.name {
        return RecipeLabel::Named { name: name.clone() };
    }
    let (Some(op), Some(repository)) = (recipe.op(), view.origin.repository()) else {
        return RecipeLabel::Repository {
            repository: view.origin.name().clone(),
        };
    };
    let branch = || RecipeLabelHead::Revision {
        revision: head_revision(&repository.branch),
    };
    let (base, head) = match op {
        RecipeOp::Diff { target } => match target {
            RecipeTarget::Unpushed { pinned } => (
                comparison
                    .cloned()
                    .or_else(|| pinned.as_ref().map(|pin| short_commit(&pin.base)))
                    .unwrap_or_else(|| repository.upstream.clone()),
                branch(),
            ),
            RecipeTarget::Base { rev } => (short_revision(rev), RecipeLabelHead::WorkingTree),
            RecipeTarget::Commit { rev } => (
                short_revision(&rev.first_parent()),
                RecipeLabelHead::Revision {
                    revision: short_revision(rev),
                },
            ),
            RecipeTarget::Range { range, .. } => range_endpoints(range).map_or_else(
                || (GitRevision::from(range), branch()),
                |(base, head)| (base, RecipeLabelHead::Revision { revision: head }),
            ),
            RecipeTarget::Merge { base, .. } => (base.clone(), branch()),
            RecipeTarget::Last { count, pinned } => (
                pinned.as_ref().map_or_else(
                    || GitRevision::head_ancestor(*count),
                    |pin| short_commit(&pin.base),
                ),
                branch(),
            ),
        },
        RecipeOp::MergeDiff { base, .. } => {
            (base.clone().unwrap_or_else(GitRevision::main), branch())
        }
    };
    RecipeLabel::Compared {
        repository: repository.name.clone(),
        base,
        head,
    }
}

fn head_revision(head: &GitHead) -> GitRevision {
    match head {
        GitHead::Branch(branch) => {
            GitRevision::try_new(branch.to_string()).unwrap_or_else(|_| GitRevision::head())
        }
        GitHead::Detached => GitRevision::head(),
    }
}

/// Characters a tab label keeps of a commit ID; the label already names the repository.
const TAB_COMMIT_ID_CHARACTERS: usize = 6;

fn short_commit(id: &CommitId) -> GitRevision {
    short_revision(&GitRevision::from(id))
}

/// Shortens a revision written as a commit ID; branch, tag, and relative names stay as written.
fn short_revision(revision: &GitRevision) -> GitRevision {
    let text = revision.as_ref();
    if text.len() > TAB_COMMIT_ID_CHARACTERS && text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        GitRevision::try_new(text[..TAB_COMMIT_ID_CHARACTERS].to_owned())
            .unwrap_or_else(|_| revision.clone())
    } else {
        revision.clone()
    }
}

fn range_endpoints(range: &GitRange) -> Option<(GitRevision, GitRevision)> {
    let text = range.as_ref();
    let (base, head) = text.split_once("...").or_else(|| text.split_once(".."))?;
    Some((
        short_revision(&GitRevision::try_new(base.to_owned()).ok()?),
        short_revision(&GitRevision::try_new(head.to_owned()).ok()?),
    ))
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
        RecipeTarget::Commit { rev } => RecipeLabelChanges::Commit { rev: rev.clone() },
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
    use crate::utils::{
        diffs::commit,
        git_range, git_revision, project_name, repository_root,
        viewer::{empty_view, recipe},
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
            branch: crate::utils::diffs::repository_origin(&view).branch.clone(),
            upstream: crate::utils::diffs::repository_origin(&view)
                .upstream
                .clone(),
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
                rendered(&recipe, view.origin.name().clone(), parts),
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
    }

    #[test]
    fn root_repository_uses_its_full_path_as_the_label() {
        let mut root = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        crate::utils::viewer::set_root(
            &mut root,
            repository_root(if cfg!(windows) { r"C:\" } else { "/" }),
        );

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
