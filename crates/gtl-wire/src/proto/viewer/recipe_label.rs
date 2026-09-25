use std::num::NonZeroU32;

use gtl_models::{
    git::{CommitCount, GitHead, GitRange, GitRevision},
    paths::ProjectName,
    recipes::{RecipeLabel, RecipeLabelChanges},
};

use super::{ViewerCodecError, required};
use crate::v1;

pub(super) fn encode(label: RecipeLabel) -> v1::ViewerRecipeLabel {
    use v1::viewer_recipe_label::Label;

    let label = match label {
        RecipeLabel::Named { name } => Label::Name(name.to_string()),
        RecipeLabel::Repository { repository } => Label::Repository(repository.to_string()),
        RecipeLabel::Changes {
            repository,
            changes,
        } => Label::Changes(v1::ViewerRecipeChangesLabel {
            repository: repository.to_string(),
            changes: Some(encode_changes(changes)),
        }),
    };
    v1::ViewerRecipeLabel { label: Some(label) }
}

fn encode_changes(changes: RecipeLabelChanges) -> v1::viewer_recipe_changes_label::Changes {
    use v1::viewer_recipe_changes_label::Changes;

    match changes {
        RecipeLabelChanges::Unpushed => Changes::Unpushed(v1::Empty {}),
        RecipeLabelChanges::UnpushedCommits { count } => {
            Changes::UnpushedCommitCount(count.into_inner())
        }
        RecipeLabelChanges::WorkingTree { base } => Changes::WorkingTreeBase(base.to_string()),
        RecipeLabelChanges::Range { range } => Changes::Range(range.to_string()),
        RecipeLabelChanges::MergeInto { base } => Changes::MergeIntoBase(base.to_string()),
        RecipeLabelChanges::Merge { branch, upstream } => {
            Changes::Merge(v1::ViewerRecipeMergeLabel {
                branch: branch.to_string(),
                upstream: upstream.to_string(),
            })
        }
        RecipeLabelChanges::LastCommits { count } => Changes::LastCommitCount(count.get()),
    }
}

pub(super) fn decode(label: v1::ViewerRecipeLabel) -> Result<RecipeLabel, ViewerCodecError> {
    use v1::viewer_recipe_label::Label;

    Ok(match required(label.label)? {
        Label::Name(name) => RecipeLabel::Named {
            name: project_name(name)?,
        },
        Label::Repository(repository) => RecipeLabel::Repository {
            repository: project_name(repository)?,
        },
        Label::Changes(changes) => RecipeLabel::Changes {
            repository: project_name(changes.repository)?,
            changes: decode_changes(required(changes.changes)?)?,
        },
    })
}

fn decode_changes(
    changes: v1::viewer_recipe_changes_label::Changes,
) -> Result<RecipeLabelChanges, ViewerCodecError> {
    use v1::viewer_recipe_changes_label::Changes;

    Ok(match changes {
        Changes::Unpushed(v1::Empty {}) => RecipeLabelChanges::Unpushed,
        Changes::UnpushedCommitCount(count) => RecipeLabelChanges::UnpushedCommits {
            count: CommitCount::new(count),
        },
        Changes::WorkingTreeBase(base) => RecipeLabelChanges::WorkingTree {
            base: revision(base)?,
        },
        Changes::Range(range) => RecipeLabelChanges::Range {
            range: GitRange::try_new(range).map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
        Changes::MergeIntoBase(base) => RecipeLabelChanges::MergeInto {
            base: revision(base)?,
        },
        Changes::Merge(merge) => RecipeLabelChanges::Merge {
            branch: GitHead::try_from(merge.branch)
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            upstream: revision(merge.upstream)?,
        },
        Changes::LastCommitCount(count) => RecipeLabelChanges::LastCommits {
            count: NonZeroU32::new(count).ok_or(ViewerCodecError::InvalidMessage)?,
        },
    })
}

fn project_name(name: String) -> Result<ProjectName, ViewerCodecError> {
    ProjectName::try_new(name).map_err(|_| ViewerCodecError::InvalidMessage)
}

fn revision(revision: String) -> Result<GitRevision, ViewerCodecError> {
    GitRevision::try_new(revision).map_err(|_| ViewerCodecError::InvalidMessage)
}
