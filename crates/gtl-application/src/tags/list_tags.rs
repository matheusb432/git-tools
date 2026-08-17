//! Lists local tags as structured commit groups.

use std::{cmp::Ordering, collections::HashMap};

use gtl_models::{diffs::CommitId, paths::RepositoryRoot, tags::Tag};

use super::{compare_tags, git_command_error::GitCommandError, refs};
use crate::ports::GitClient;

/// Groups tags that resolve to the same commit for deterministic presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagGroup {
    /// One tag is the only local ref resolving to its commit.
    Single(Tag),
    /// One annotated tag owns the remaining lightweight labels on its commit.
    Canonical { canonical: Tag, labels: Vec<Tag> },
    /// Several annotated tags resolve to the same commit, so none is canonical.
    MoreThanOneTagHasMessage(Vec<Tag>),
    /// Only lightweight labels resolve to the commit.
    AllLabels(Vec<Tag>),
}

impl TagGroup {
    fn order_tag(&self) -> Option<&Tag> {
        match self {
            Self::Single(tag) => Some(tag),
            Self::Canonical { canonical, labels } => std::iter::once(canonical)
                .chain(labels)
                .min_by(|left, right| compare_tags(left, right)),
            Self::MoreThanOneTagHasMessage(tags) | Self::AllLabels(tags) => {
                tags.iter().min_by(|left, right| compare_tags(left, right))
            }
        }
    }
}

/// Reports either structured local tag groups or the Git failure that prevented listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListTagsOk {
    /// Git refs loaded and grouped successfully.
    Listed { groups: Vec<TagGroup> },
    /// Git refs could not be loaded.
    Failed { detail: String },
}

/// Requests the local tags for one repository, optionally with their origin state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTags {
    pub repo_path: RepositoryRoot,
    /// Queries origin over the network to resolve each tag's [`gtl_models::tags::TagState`].
    pub include_state: bool,
}

/// Reports an unexpected Git transport failure while listing tags.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ListTagsError {
    /// Git could not be started or its output could not be collected.
    #[error("{0}")]
    Unexpected(#[source] anyhow::Error),
}

/// Loads and groups local tags without applying presentation choices.
///
/// # Errors
///
/// Returns [`ListTagsError`] when Git cannot be executed.
#[cqrsy::query]
pub fn execute(query: ListTags, git: &impl GitClient) -> Result<ListTagsOk, ListTagsError> {
    let ListTags {
        repo_path,
        include_state,
    } = query;
    let refs = if include_state {
        refs::load(git, &repo_path)
    } else {
        refs::load_local(git, &repo_path)
    };
    match refs {
        Ok(refs) => Ok(ListTagsOk::Listed {
            groups: group_tags(refs.into_listed()),
        }),
        Err(GitCommandError::Rejected { detail, .. }) => Ok(ListTagsOk::Failed { detail }),
        Err(GitCommandError::Transport { source, .. }) => Err(ListTagsError::Unexpected(source)),
    }
}

fn group_tags(tags: Vec<Tag>) -> Vec<TagGroup> {
    let mut by_commit = HashMap::<CommitId, Vec<Tag>>::new();
    for tag in tags {
        by_commit.entry(tag.commit().clone()).or_default().push(tag);
    }

    let mut groups = by_commit
        .into_values()
        .map(classify_group)
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| match (left.order_tag(), right.order_tag()) {
        (Some(left), Some(right)) => compare_tags(left, right),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (None, None) => Ordering::Equal,
    });
    groups
}

fn classify_group(mut tags: Vec<Tag>) -> TagGroup {
    tags.sort_by(compare_tags);
    if tags.len() == 1 {
        return TagGroup::Single(tags.remove(0));
    }

    let canonical_indexes = tags
        .iter()
        .enumerate()
        .filter_map(|(index, tag)| tag.is_annotated().then_some(index))
        .collect::<Vec<_>>();
    match canonical_indexes.as_slice() {
        [canonical_index] => {
            let canonical = tags.remove(*canonical_index);
            TagGroup::Canonical {
                canonical,
                labels: tags,
            }
        }
        [] => TagGroup::AllLabels(tags),
        _ => TagGroup::MoreThanOneTagHasMessage(tags),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use gtl_models::tags::Tag;

    use super::{ListTags, TagGroup, group_tags};
    use crate::{
        tags::{ListTagsOk, list_tags},
        utils::ScriptedGitClient,
    };

    fn annotated_tag(
        name: &str,
        object: &str,
        commit: &str,
        created_at: i64,
        message: &str,
    ) -> Tag {
        Tag::annotated(
            crate::utils::tag_name(name),
            crate::utils::git_object_id(object),
            crate::utils::commit_id_fixture(commit),
            Some(
                gtl_models::timestamps::MachineTimestamp::from_unix_seconds(created_at)
                    .expect("fixture tag timestamp is in range"),
            ),
            Some(message.into()),
        )
    }

    fn lightweight_tag(name: &str, commit: &str, created_at: i64) -> Tag {
        Tag::lightweight(
            crate::utils::tag_name(name),
            crate::utils::commit_id_fixture(commit),
            Some(
                gtl_models::timestamps::MachineTimestamp::from_unix_seconds(created_at)
                    .expect("fixture tag timestamp is in range"),
            ),
        )
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = list_tags::execute(list_with_state(), &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn nonzero_exit_remains_the_exact_closed_failure() {
        let git =
            ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: refs unavailable")]);

        assert_eq!(
            list_tags::execute(list_with_state(), &git)
                .expect("a Git rejection is a closed list failure"),
            ListTagsOk::Failed {
                detail: "git for-each-ref failed: fatal: refs unavailable".into(),
            }
        );
    }

    #[test]
    fn listing_without_state_never_contacts_origin() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        list_tags::execute(
            ListTags {
                repo_path: crate::utils::repository_root("/repo"),
                include_state: false,
            },
            &git,
        )
        .expect("scripted git succeeds");
    }

    #[test]
    fn unreachable_origin_is_a_closed_failure_naming_the_remote_query() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("fatal: 'origin' does not appear to be a git repository"),
        ]);

        assert_eq!(
            list_tags::execute(list_with_state(), &git)
                .expect("a Git rejection is a closed list failure"),
            ListTagsOk::Failed {
                detail:
                    "git ls-remote failed: fatal: 'origin' does not appear to be a git repository"
                        .into(),
            }
        );
    }

    #[test]
    fn one_annotated_tag_owns_lightweight_labels_on_the_same_commit() {
        let canonical = annotated_tag("v1.0.0", "tag-object", "commit-a", 100, "release");
        let label = lightweight_tag("stable", "commit-a", 110);

        assert_eq!(
            group_tags(vec![canonical.clone(), label.clone()]),
            vec![TagGroup::Canonical {
                canonical,
                labels: vec![label],
            }]
        );
    }

    #[test]
    fn multiple_annotated_tags_stay_complete_under_their_commit() {
        let first = annotated_tag("v1.0.0", "tag-a", "commit-a", 100, "release one");
        let second = annotated_tag("v1.1.0", "tag-b", "commit-a", 110, "release two");

        assert_eq!(
            group_tags(vec![first.clone(), second.clone()]),
            vec![TagGroup::MoreThanOneTagHasMessage(vec![first, second])]
        );
    }

    #[test]
    fn lightweight_tags_stay_complete_under_their_commit() {
        let first = lightweight_tag("alpha", "commit-a", 100);
        let second = lightweight_tag("stable", "commit-a", 110);

        assert_eq!(
            group_tags(vec![first.clone(), second.clone()]),
            vec![TagGroup::AllLabels(vec![first, second])]
        );
    }

    #[test]
    fn annotated_tag_with_an_empty_subject_remains_canonical() {
        let canonical = Tag::annotated(
            crate::utils::tag_name("v1.0.0"),
            crate::utils::git_object_id("tag-object"),
            crate::utils::commit_id_fixture("commit-a"),
            Some(
                gtl_models::timestamps::MachineTimestamp::from_unix_seconds(100)
                    .expect("fixture tag timestamp is in range"),
            ),
            None,
        );
        let label = lightweight_tag("stable", "commit-a", 110);

        assert_eq!(
            group_tags(vec![canonical.clone(), label.clone()]),
            vec![TagGroup::Canonical {
                canonical,
                labels: vec![label],
            }]
        );
    }

    #[test]
    fn creation_date_ties_order_version_tags_by_numeric_segments() {
        let ninth = lightweight_tag("v0.9.0", "commit-a", 100);
        let tenth = lightweight_tag("v0.10.0", "commit-b", 100);
        let second = lightweight_tag("v0.2.0", "commit-c", 100);

        assert_eq!(
            group_tags(vec![tenth.clone(), ninth.clone(), second.clone()]),
            vec![
                TagGroup::Single(second),
                TagGroup::Single(ninth),
                TagGroup::Single(tenth),
            ]
        );
    }

    #[test]
    fn tags_inside_one_group_order_like_the_groups_themselves() {
        let ninth = annotated_tag("v0.9.0", "tag-a", "commit-a", 100, "ninth");
        let tenth = annotated_tag("v0.10.0", "tag-b", "commit-a", 100, "tenth");
        let second = annotated_tag("v0.2.0", "tag-c", "commit-a", 100, "second");

        assert_eq!(
            group_tags(vec![tenth.clone(), ninth.clone(), second.clone()]),
            vec![TagGroup::MoreThanOneTagHasMessage(vec![
                second, ninth, tenth
            ])]
        );
    }

    #[test]
    fn groups_sort_by_earliest_creation_then_tag_name() {
        let later = lightweight_tag("alpha", "commit-b", 200);
        let tie_second = lightweight_tag("zeta", "commit-c", 100);
        let tie_first = lightweight_tag("beta", "commit-a", 100);

        assert_eq!(
            group_tags(vec![later.clone(), tie_second.clone(), tie_first.clone()]),
            vec![
                TagGroup::Single(tie_first),
                TagGroup::Single(tie_second),
                TagGroup::Single(later),
            ]
        );
    }

    fn list_with_state() -> ListTags {
        ListTags {
            repo_path: crate::utils::repository_root("/repo"),
            include_state: true,
        }
    }
}
