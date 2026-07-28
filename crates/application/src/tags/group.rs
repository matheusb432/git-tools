//! Pure grouping and ordering of local tag values.

use std::{cmp::Ordering, collections::BTreeMap};

use domain::tags::Tag;

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
pub enum ListTagsOk {
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
            commit_order.push(tag.commit().to_string());
        }
        by_commit
            .entry(tag.commit().to_string())
            .or_default()
            .push(tag);
    }

    let mut groups = commit_order
        .into_iter()
        .map(|commit| classify(by_commit.remove(&commit).expect("commit key was inserted")))
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| compare_tags(left.order_tag(), right.order_tag()));
    groups
}

fn classify(mut tags: Vec<Tag>) -> TagGroup {
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

pub(super) fn compare_tags(left: &Tag, right: &Tag) -> Ordering {
    left.created_at()
        .cmp(&right.created_at())
        .then_with(|| natord::compare(left.name(), right.name()))
}

#[cfg(test)]
mod tests {
    use domain::tags::Tag;

    use super::{TagGroup, group};

    fn annotated_tag(
        name: &str,
        object: &str,
        commit: &str,
        created_at: i64,
        message: &str,
    ) -> Tag {
        Tag::annotated(
            name.into(),
            object.into(),
            commit.into(),
            commit.into(),
            Some(created_at),
            Some(message.into()),
        )
    }

    fn lightweight_tag(name: &str, commit: &str, created_at: i64) -> Tag {
        Tag::lightweight(name.into(), commit.into(), commit.into(), Some(created_at))
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
        let canonical = Tag::annotated(
            "v1.0.0".into(),
            "tag-object".into(),
            "commit-a".into(),
            "commit-a".into(),
            Some(100),
            None,
        );
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
    fn creation_date_ties_order_version_tags_by_numeric_segments() {
        let ninth = lightweight_tag("v0.9.0", "commit-a", 100);
        let tenth = lightweight_tag("v0.10.0", "commit-b", 100);
        let second = lightweight_tag("v0.2.0", "commit-c", 100);

        assert_eq!(
            group(vec![tenth.clone(), ninth.clone(), second.clone()]),
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
            group(vec![tenth.clone(), ninth.clone(), second.clone()]),
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
            group(vec![later.clone(), tie_second.clone(), tie_first.clone()]),
            vec![
                TagGroup::Single(tie_first),
                TagGroup::Single(tie_second),
                TagGroup::Single(later),
            ]
        );
    }
}
