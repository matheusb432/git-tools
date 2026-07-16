use std::{cmp::Ordering, collections::BTreeMap};

/// Identifies whether a local tag object is known by the origin tracking refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagState {
    /// The tag object is absent from origin's local tracking refs.
    Local,
    /// Origin's local tracking ref points to the same tag object.
    Remote,
}

/// Describes one local tag and the commit it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub(super) object: String,
    pub(super) commit: String,
    pub(super) commit_short: String,
    pub(super) name: String,
    pub(super) created_at: Option<i64>,
    pub(super) message: Option<String>,
    pub(super) annotated: bool,
    pub(super) state: TagState,
}

impl Tag {
    /// Returns the full object identifier stored in the tag ref.
    pub(super) fn object(&self) -> &str {
        &self.object
    }

    /// Returns the full commit identifier the tag resolves to.
    pub(super) fn commit(&self) -> &str {
        &self.commit
    }

    /// Returns the abbreviated commit identifier used for presentation.
    pub fn commit_short(&self) -> &str {
        &self.commit_short
    }

    /// Returns the local tag name without the `refs/tags/` prefix.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the first line of an annotated tag message.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Returns whether the tag is local-only or known by origin.
    pub fn state(&self) -> TagState {
        self.state
    }

    fn is_annotated(&self) -> bool {
        self.annotated
    }
}

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
    fn order_tag(&self) -> &Tag {
        self.tags()
            .min_by(|left, right| compare_tags(left, right))
            .expect("every tag group contains at least one tag")
    }

    fn tags(&self) -> impl Iterator<Item = &Tag> {
        let (first, rest) = match self {
            Self::Single(tag) => (tag, [].as_slice()),
            Self::Canonical { canonical, labels } => (canonical, labels.as_slice()),
            Self::MoreThanOneTagHasMessage(tags) | Self::AllLabels(tags) => {
                let (first, rest) = tags
                    .split_first()
                    .expect("group creates only non-empty tag groups");
                (first, rest)
            }
        };
        std::iter::once(first).chain(rest)
    }
}

/// Reports either structured local tag groups or the Git failure that prevented listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagList {
    /// Git refs loaded and grouped successfully.
    Listed { groups: Vec<TagGroup> },
    /// Git refs could not be loaded.
    Failed { detail: String },
}

pub(super) fn group(tags: Vec<Tag>) -> Vec<TagGroup> {
    let mut by_commit = BTreeMap::<String, Vec<Tag>>::new();
    let mut commit_order = Vec::new();
    for tag in tags {
        if !by_commit.contains_key(tag.commit()) {
            commit_order.push(tag.commit.clone());
        }
        by_commit.entry(tag.commit.clone()).or_default().push(tag);
    }

    let mut groups = commit_order
        .into_iter()
        .map(|commit| classify(by_commit.remove(&commit).expect("commit key was inserted")))
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| compare_tags(left.order_tag(), right.order_tag()));
    groups
}

fn classify(mut tags: Vec<Tag>) -> TagGroup {
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

fn compare_tags(left: &Tag, right: &Tag) -> Ordering {
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| left.name.cmp(&right.name))
}

#[cfg(test)]
mod tests {
    use super::{Tag, TagGroup, TagState, group};

    fn annotated_tag(
        name: &str,
        object: &str,
        commit: &str,
        created_at: i64,
        message: &str,
    ) -> Tag {
        Tag {
            object: object.into(),
            commit: commit.into(),
            commit_short: commit.into(),
            name: name.into(),
            created_at: Some(created_at),
            message: Some(message.into()),
            annotated: true,
            state: TagState::Local,
        }
    }

    fn lightweight_tag(name: &str, commit: &str, created_at: i64) -> Tag {
        Tag {
            object: commit.into(),
            commit: commit.into(),
            commit_short: commit.into(),
            name: name.into(),
            created_at: Some(created_at),
            message: None,
            annotated: false,
            state: TagState::Local,
        }
    }

    #[test]
    fn one_annotated_tag_owns_lightweight_labels_on_the_same_commit() {
        let canonical = annotated_tag("v1.0.0", "tag-object", "commit-a", 100, "release");
        let label = lightweight_tag("stable", "commit-a", 110);
        assert_eq!(
            group(vec![canonical.clone(), label.clone()]),
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
            group(vec![first.clone(), second.clone()]),
            vec![TagGroup::MoreThanOneTagHasMessage(vec![first, second])]
        );
    }

    #[test]
    fn lightweight_tags_stay_complete_under_their_commit() {
        let first = lightweight_tag("alpha", "commit-a", 100);
        let second = lightweight_tag("stable", "commit-a", 110);

        assert_eq!(
            group(vec![first.clone(), second.clone()]),
            vec![TagGroup::AllLabels(vec![first, second])]
        );
    }

    #[test]
    fn annotated_tag_with_an_empty_subject_remains_canonical() {
        let mut canonical = annotated_tag("v1.0.0", "tag-object", "commit-a", 100, "");
        canonical.message = None;
        let label = lightweight_tag("stable", "commit-a", 110);

        assert_eq!(
            group(vec![canonical.clone(), label.clone()]),
            vec![TagGroup::Canonical {
                canonical,
                labels: vec![label],
            }]
        );
    }

    #[test]
    fn groups_sort_by_earliest_creation_then_tag_name() {
        let later = lightweight_tag("alpha", "commit-b", 200);
        let tie_second = lightweight_tag("zeta", "commit-c", 100);
        let tie_first = lightweight_tag("beta", "commit-a", 100);

        assert_eq!(
            group(vec![later.clone(), tie_second.clone(), tie_first.clone()]),
            vec![
                TagGroup::Single(tie_first),
                TagGroup::Single(tie_second),
                TagGroup::Single(later),
            ]
        );
    }
}
