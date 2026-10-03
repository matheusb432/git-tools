use std::collections::BTreeMap;

use gtl_models::{
    git::{GitObjectId, RemoteName, TagName},
    paths::RepositoryRoot,
    tags::{Tag, TagState},
};

use super::{compare_tags, git_command_error::GitCommandError};
use crate::ports::{GitClient, GitEffect};

pub(super) struct TagRefs {
    local: BTreeMap<TagName, Tag>,
    /// Tag name to the object id origin's `refs/tags/<name>` points at, or
    /// `None` when origin was not queried.
    remote: Option<BTreeMap<TagName, GitObjectId>>,
}

impl TagRefs {
    pub(super) fn new(
        local: BTreeMap<TagName, Tag>,
        remote: Option<BTreeMap<TagName, GitObjectId>>,
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

    pub(super) fn local(&self, name: &TagName) -> Option<&Tag> {
        self.local.get(name)
    }

    pub(super) fn into_listed(mut self) -> Vec<Tag> {
        let Some(remote) = &self.remote else {
            return self.local.into_values().collect();
        };
        for tag in self.local.values_mut() {
            tag.set_state(listed_state(remote, tag));
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

fn listed_state(remote: &BTreeMap<TagName, GitObjectId>, tag: &Tag) -> TagState {
    if remote
        .get(tag.name())
        .is_some_and(|object| object == tag.object())
    {
        TagState::Remote
    } else {
        TagState::Local
    }
}

pub(super) fn load(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(
        local_refs(git, repo_path)?,
        Some(remote_refs(git, repo_path)?),
    ))
}

pub(super) fn load_local(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(local_refs(git, repo_path)?, None))
}

pub(super) fn local_refs(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<BTreeMap<TagName, Tag>, GitCommandError> {
    match git.local_tags(repo_path)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

fn remote_refs(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<BTreeMap<TagName, GitObjectId>, GitCommandError> {
    match git.remote_tags(repo_path, &RemoteName::origin())? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git ls-remote failed: {detail}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gtl_models::{
        git::{GitObjectId, TagName},
        tags::{Tag, TagState},
    };

    use super::TagRefs;

    fn local_tag(name: &str, object: &str, created_at: i64) -> Tag {
        Tag::lightweight(
            crate::utils::tag_name(name),
            crate::utils::commit_id_fixture(object),
            Some(gtl_models::timestamps::MachineTimestamp::from_unix_seconds(created_at).unwrap()),
        )
    }

    fn local_entry(name: &str, object: &str, created_at: i64) -> (TagName, Tag) {
        (
            crate::utils::tag_name(name),
            local_tag(name, object, created_at),
        )
    }

    fn remote_entry(name: &str, object: &str) -> (TagName, GitObjectId) {
        (
            crate::utils::tag_name(name),
            crate::utils::git_object_id(object),
        )
    }

    #[test]
    fn pending_tags_keep_the_listing_order_not_the_ref_name_order() {
        let local = BTreeMap::from([
            local_entry("v0.10.0", "object-a", 100),
            local_entry("v0.2.0", "object-b", 100),
            local_entry("v0.9.0", "object-c", 100),
        ]);
        let refs = TagRefs::new(local, Some(BTreeMap::new()));

        assert_eq!(
            refs.pending()
                .into_iter()
                .map(|tag| tag.name().to_string())
                .collect::<Vec<_>>(),
            vec!["v0.2.0", "v0.9.0", "v0.10.0"]
        );
    }

    #[test]
    fn pending_tags_are_local_refs_not_known_at_the_same_remote_object() {
        let local = BTreeMap::from([
            local_entry("v1.0.0", "object-a", 100),
            local_entry("v1.1.0", "object-b", 110),
        ]);
        let remote = BTreeMap::from([
            remote_entry("v1.0.0", "object-a"),
            remote_entry("v1.1.0", "old-object"),
        ]);
        let refs = TagRefs::new(local, Some(remote));

        assert_eq!(
            refs.pending()
                .into_iter()
                .map(|tag| tag.name().to_string())
                .collect::<Vec<_>>(),
            vec!["v1.1.0"]
        );
    }

    #[test]
    fn listing_without_a_remote_query_leaves_every_tag_state_unknown() {
        let local = BTreeMap::from([local_entry("v1.0.0", "object-a", 100)]);

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
            local_entry("v1.0.0", "object-a", 100),
            local_entry("v1.1.0", "object-b", 110),
        ]);
        let remote = BTreeMap::from([
            remote_entry("v1.0.0", "object-a"),
            remote_entry("v1.1.0", "old-object"),
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
