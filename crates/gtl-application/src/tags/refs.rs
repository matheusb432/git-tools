use std::{collections::BTreeMap, path::Path};

use gtl_models::tags::{Tag, TagState};

use super::{compare_tags, git_command_error::GitCommandError};
use crate::ports::{GitClient, GitEffect};

pub(super) struct TagRefs {
    local: BTreeMap<String, Tag>,
    /// Tag name to the object id origin's `refs/tags/<name>` points at, or
    /// `None` when origin was not queried.
    remote: Option<BTreeMap<String, String>>,
}

impl TagRefs {
    pub(super) fn new(
        local: BTreeMap<String, Tag>,
        remote: Option<BTreeMap<String, String>>,
    ) -> Self {
        Self { local, remote }
    }

    pub(super) fn pending(&self) -> Vec<&Tag> {
        let mut pending = self
            .local
            .values()
            .filter(|tag| !self.is_remote(tag))
            .collect::<Vec<_>>();
        pending.sort_by(|left, right| compare_tags(left, right));
        pending
    }

    pub(super) fn local(&self, name: &str) -> Option<&Tag> {
        self.local.get(name)
    }

    pub(super) fn into_listed(mut self) -> Vec<Tag> {
        if let Some(remote) = &self.remote {
            for tag in self.local.values_mut() {
                let state = if remote
                    .get(tag.name())
                    .is_some_and(|object| object == tag.object())
                {
                    TagState::Remote
                } else {
                    TagState::Local
                };
                tag.set_state(state);
            }
        }
        self.local.into_values().collect()
    }

    pub(super) fn is_remote(&self, tag: &Tag) -> bool {
        self.remote.as_ref().is_some_and(|remote| {
            remote
                .get(tag.name())
                .is_some_and(|object| object == tag.object())
        })
    }
}

pub(super) fn load(git: &impl GitClient, repo_path: &Path) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(
        local_refs(git, repo_path)?,
        Some(remote_refs(git, repo_path)?),
    ))
}

pub(super) fn load_local(
    git: &impl GitClient,
    repo_path: &Path,
) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(local_refs(git, repo_path)?, None))
}

fn local_refs(
    git: &impl GitClient,
    repo_path: &Path,
) -> Result<BTreeMap<String, Tag>, GitCommandError> {
    match git.local_tags(repo_path)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

fn remote_refs(
    git: &impl GitClient,
    repo_path: &Path,
) -> Result<BTreeMap<String, String>, GitCommandError> {
    match git.remote_tags(repo_path, "origin")? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git ls-remote failed: {detail}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gtl_models::tags::{Tag, TagState};

    use super::TagRefs;

    fn local_tag(name: &str, object: &str, created_at: i64) -> Tag {
        Tag::lightweight(
            name.into(),
            object.into(),
            object.chars().take(7).collect(),
            Some(created_at),
        )
    }

    #[test]
    fn pending_tags_keep_the_listing_order_not_the_ref_name_order() {
        let local = BTreeMap::from([
            ("v0.10.0".into(), local_tag("v0.10.0", "object-a", 100)),
            ("v0.2.0".into(), local_tag("v0.2.0", "object-b", 100)),
            ("v0.9.0".into(), local_tag("v0.9.0", "object-c", 100)),
        ]);
        let refs = TagRefs::new(local, Some(BTreeMap::new()));

        assert_eq!(
            refs.pending()
                .into_iter()
                .map(Tag::name)
                .collect::<Vec<_>>(),
            vec!["v0.2.0", "v0.9.0", "v0.10.0"]
        );
    }

    #[test]
    fn pending_tags_are_local_refs_not_known_at_the_same_remote_object() {
        let local = BTreeMap::from([
            ("v1.0.0".into(), local_tag("v1.0.0", "object-a", 100)),
            ("v1.1.0".into(), local_tag("v1.1.0", "object-b", 110)),
        ]);
        let remote = BTreeMap::from([
            ("v1.0.0".into(), "object-a".into()),
            ("v1.1.0".into(), "old-object".into()),
        ]);
        let refs = TagRefs::new(local, Some(remote));

        assert_eq!(
            refs.pending()
                .into_iter()
                .map(Tag::name)
                .collect::<Vec<_>>(),
            vec!["v1.1.0"]
        );
    }

    #[test]
    fn listing_without_a_remote_query_leaves_every_tag_state_unknown() {
        let local = BTreeMap::from([("v1.0.0".into(), local_tag("v1.0.0", "object-a", 100))]);

        let states = TagRefs::new(local, None)
            .into_listed()
            .into_iter()
            .map(|tag| tag.state())
            .collect::<Vec<_>>();

        assert_eq!(states, vec![None]);
    }

    #[test]
    fn listing_marks_tags_remote_only_when_origin_holds_the_same_object() {
        let local = BTreeMap::from([
            ("v1.0.0".into(), local_tag("v1.0.0", "object-a", 100)),
            ("v1.1.0".into(), local_tag("v1.1.0", "object-b", 110)),
        ]);
        let remote = BTreeMap::from([
            ("v1.0.0".into(), "object-a".into()),
            ("v1.1.0".into(), "old-object".into()),
        ]);

        let states = TagRefs::new(local, Some(remote))
            .into_listed()
            .into_iter()
            .map(|tag| (tag.name().to_string(), tag.state()))
            .collect::<Vec<_>>();

        assert_eq!(
            states,
            vec![
                ("v1.0.0".into(), Some(TagState::Remote)),
                ("v1.1.0".into(), Some(TagState::Local)),
            ]
        );
    }
}
