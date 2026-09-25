use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{
    git::{CommitCount, GitHead, GitRange, GitRevision},
    paths::ProjectName,
};

/// Names a recipe's diff in tabs, feedback, and history in parts the viewer localizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "label", rename_all = "snake_case")]
pub enum RecipeLabel {
    /// Shows the name the user gave the recipe.
    Named { name: ProjectName },
    /// Names a live view by the repository it follows.
    Repository { repository: ProjectName },
    /// Describes which changes the recipe shows in its repository.
    Changes {
        repository: ProjectName,
        changes: RecipeLabelChanges,
    },
}

/// Describes the changes a recipe compares, with the facts known so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "changes", rename_all = "snake_case")]
pub enum RecipeLabelChanges {
    /// Unpushed commits whose count is not computed yet.
    Unpushed,
    /// Unpushed commits with their computed count.
    UnpushedCommits { count: CommitCount },
    /// The working tree compared with a base revision.
    WorkingTree { base: GitRevision },
    /// The endpoints of an explicit revision range.
    Range { range: GitRange },
    /// A merge into a base whose merged branch is not computed yet.
    MergeInto { base: GitRevision },
    /// A merge of the current branch into its computed upstream.
    Merge {
        branch: GitHead,
        upstream: GitRevision,
    },
    /// The latest commits.
    LastCommits { count: NonZeroU32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_serialize_their_parts_under_snake_case_tags() {
        let label = RecipeLabel::Changes {
            repository: ProjectName::try_new("git-tools").unwrap(),
            changes: RecipeLabelChanges::Merge {
                branch: GitHead::try_from("feature".to_owned()).unwrap(),
                upstream: GitRevision::try_new("origin/main").unwrap(),
            },
        };

        let json = serde_json::to_value(&label).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "label": "changes",
                "repository": "git-tools",
                "changes": {
                    "changes": "merge",
                    "branch": "feature",
                    "upstream": "origin/main",
                },
            })
        );
        assert_eq!(serde_json::from_value::<RecipeLabel>(json).unwrap(), label);
    }

    #[test]
    fn decoding_rejects_empty_label_parts() {
        let json = serde_json::json!({ "label": "named", "name": "" });

        assert!(serde_json::from_value::<RecipeLabel>(json).is_err());
    }
}
