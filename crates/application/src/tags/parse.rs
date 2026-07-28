use std::{collections::BTreeMap, path::Path};

use domain::tags::{Tag, TagState};

use super::{git_command_error::GitCommandError, group};
use crate::ports::{GitClient, GitEffect};

#[cfg(any(test, feature = "testing"))]
pub(crate) const LOCAL_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)";

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
        pending.sort_by(|left, right| group::compare_tags(left, right));
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

pub(super) fn load(git: &impl GitClient, repo: &Path) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(
        local_refs(git, repo)?,
        Some(remote_refs(git, repo)?),
    ))
}

pub(super) fn load_local(git: &impl GitClient, repo: &Path) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(local_refs(git, repo)?, None))
}

fn local_refs(git: &impl GitClient, repo: &Path) -> Result<BTreeMap<String, Tag>, GitCommandError> {
    match git.local_tags(repo)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

fn remote_refs(
    git: &impl GitClient,
    repo: &Path,
) -> Result<BTreeMap<String, String>, GitCommandError> {
    match git.remote_tags(repo, "origin")? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git ls-remote failed: {detail}"
        ))),
    }
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn parse_remote_refs(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (object, refname) = line.split_once('\t')?;
            let name = refname.strip_prefix("refs/tags/")?;
            if object.is_empty() || name.is_empty() || name.ends_with("^{}") {
                return None;
            }
            Some((name.to_string(), object.to_string()))
        })
        .collect()
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn parse_refs(stdout: &str) -> BTreeMap<String, Tag> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\t');
            let object = fields.next()?.to_string();
            let peeled_commit = fields.next()?.to_string();
            let peeled_short = fields.next()?.to_string();
            let name = fields.next()?.to_string();
            let message_and_date = fields.next().unwrap_or_default();
            let (message, created_at) = match message_and_date.rsplit_once('\t') {
                Some((message, created_at)) => match created_at.parse() {
                    Ok(created_at) => (message, Some(created_at)),
                    Err(_) => (message_and_date, None),
                },
                None => (message_and_date, None),
            };
            if object.is_empty() || name.is_empty() {
                return None;
            }

            let annotated = !peeled_commit.is_empty();
            let commit = if annotated {
                peeled_commit
            } else {
                object.clone()
            };
            let commit_short = if peeled_short.is_empty() {
                object.chars().take(7).collect()
            } else {
                peeled_short
            };
            let message = annotated
                .then(|| message.trim().to_string())
                .filter(|message| !message.is_empty());
            let tag = if annotated {
                Tag::annotated(
                    name.clone(),
                    object,
                    commit,
                    commit_short,
                    created_at,
                    message,
                )
            } else {
                Tag::lightweight(name.clone(), commit, commit_short, created_at)
            };
            Some((name, tag))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use domain::tags::{Tag, TagState};

    use super::{TagRefs, parse_refs, parse_remote_refs};

    #[test]
    fn parses_annotated_and_lightweight_refs_without_borrowing_commit_messages() {
        let refs = parse_refs(
            "tag-object\tcommit-a\tcommit\tv1.0.0\trelease one\t100\n\
             commit-b\t\t\tstable\tunderlying commit subject\t110\n",
        );

        assert_eq!(
            refs.get("v1.0.0"),
            Some(&Tag::annotated(
                "v1.0.0".into(),
                "tag-object".into(),
                "commit-a".into(),
                "commit".into(),
                Some(100),
                Some("release one".into()),
            ))
        );
        assert_eq!(
            refs.get("stable"),
            Some(&Tag::lightweight(
                "stable".into(),
                "commit-b".into(),
                "commit-".into(),
                Some(110),
            ))
        );
    }

    #[test]
    fn preserves_tabs_in_the_tag_message_before_the_final_creation_date() {
        let refs = parse_refs("tag-object\tcommit-a\tcommit\tv1.0.0\trelease\twith tab\t100\n");

        assert_eq!(
            refs.get("v1.0.0").and_then(Tag::message),
            Some("release\twith tab")
        );
    }

    #[test]
    fn remote_refs_map_tag_names_to_ref_objects_and_skip_peeled_entries() {
        let refs = parse_remote_refs(
            "tag-object\trefs/tags/v1.0.0\n\
             commit-a\trefs/tags/v1.0.0^{}\n\
             commit-b\trefs/tags/stable\n\
             head-object\trefs/heads/main\n",
        );

        assert_eq!(
            refs,
            BTreeMap::from([
                ("v1.0.0".into(), "tag-object".into()),
                ("stable".into(), "commit-b".into()),
            ])
        );
    }

    #[test]
    fn pending_tags_keep_the_listing_order_not_the_ref_name_order() {
        let local = parse_refs(
            "object-a\t\t\tv0.10.0\t\t100\n\
             object-b\t\t\tv0.2.0\t\t100\n\
             object-c\t\t\tv0.9.0\t\t100\n",
        );
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
        let local = parse_refs("object-a\t\t\tv1.0.0\t\t100\nobject-b\t\t\tv1.1.0\t\t110\n");
        let remote =
            parse_remote_refs("object-a\trefs/tags/v1.0.0\nold-object\trefs/tags/v1.1.0\n");
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
        let local = parse_refs("object-a\t\t\tv1.0.0\t\t100\n");

        let states = TagRefs::new(local, None)
            .into_listed()
            .into_iter()
            .map(|tag| tag.state())
            .collect::<Vec<_>>();

        assert_eq!(states, vec![None]);
    }

    #[test]
    fn listing_marks_tags_remote_only_when_origin_holds_the_same_object() {
        let local = parse_refs("object-a\t\t\tv1.0.0\t\t100\nobject-b\t\t\tv1.1.0\t\t110\n");
        let remote =
            parse_remote_refs("object-a\trefs/tags/v1.0.0\nold-object\trefs/tags/v1.1.0\n");

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
