use std::{collections::BTreeMap, path::Path};

use super::{
    git_command_error::GitCommandError,
    tag::{Tag, TagState},
};
use crate::ports::GitRunner;

pub(super) const LOCAL_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)";
pub(super) const REMOTE_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=4)\t%(contents:lines=1)\t%(creatordate:unix)";

pub(super) struct TagRefs {
    local: BTreeMap<String, Tag>,
    remote: BTreeMap<String, Tag>,
}

impl TagRefs {
    pub(super) fn new(local: BTreeMap<String, Tag>, remote: BTreeMap<String, Tag>) -> Self {
        Self { local, remote }
    }

    pub(super) fn pending(&self) -> Vec<&Tag> {
        self.local
            .values()
            .filter(|tag| !self.is_remote(tag))
            .collect()
    }

    pub(super) fn local(&self, name: &str) -> Option<&Tag> {
        self.local.get(name)
    }

    pub(super) fn into_listed(mut self) -> Vec<Tag> {
        let remote = &self.remote;
        for tag in self.local.values_mut() {
            if remote
                .get(tag.name())
                .is_some_and(|remote_tag| remote_tag.object() == tag.object())
            {
                tag.state = TagState::Remote;
            }
        }
        self.local.into_values().collect()
    }

    pub(super) fn is_remote(&self, tag: &Tag) -> bool {
        self.remote
            .get(tag.name())
            .is_some_and(|remote| remote.object() == tag.object())
    }
}

pub(super) fn load(git: &impl GitRunner, repo: &Path) -> Result<TagRefs, GitCommandError> {
    Ok(TagRefs::new(
        load_one(
            git,
            repo,
            &["for-each-ref", LOCAL_TAG_FORMAT_ARG, "refs/tags"],
        )?,
        load_one(
            git,
            repo,
            &[
                "for-each-ref",
                REMOTE_TAG_FORMAT_ARG,
                "refs/remotes/origin/tags",
            ],
        )?,
    ))
}

fn load_one(
    git: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> Result<BTreeMap<String, Tag>, GitCommandError> {
    let output = git.run(repo, args)?;
    if output.exit_code == 0 {
        Ok(parse_refs(&output.stdout))
    } else {
        Err(GitCommandError::rejected(
            output.fail_detail("git for-each-ref failed"),
        ))
    }
}

pub(super) fn parse_refs(stdout: &str) -> BTreeMap<String, Tag> {
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
            Some((
                name.clone(),
                Tag {
                    object,
                    commit,
                    commit_short,
                    name,
                    created_at,
                    message,
                    annotated,
                    state: TagState::Local,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{TagRefs, parse_refs};
    use crate::tags::tag::{Tag, TagState};

    #[test]
    fn parses_annotated_and_lightweight_refs_without_borrowing_commit_messages() {
        let refs = parse_refs(
            "tag-object\tcommit-a\tcommit\tv1.0.0\trelease one\t100\n\
             commit-b\t\t\tstable\tunderlying commit subject\t110\n",
        );

        assert_eq!(
            refs.get("v1.0.0"),
            Some(&Tag {
                object: "tag-object".into(),
                commit: "commit-a".into(),
                commit_short: "commit".into(),
                name: "v1.0.0".into(),
                created_at: Some(100),
                message: Some("release one".into()),
                annotated: true,
                state: TagState::Local,
            })
        );
        assert_eq!(
            refs.get("stable"),
            Some(&Tag {
                object: "commit-b".into(),
                commit: "commit-b".into(),
                commit_short: "commit-".into(),
                name: "stable".into(),
                created_at: Some(110),
                message: None,
                annotated: false,
                state: TagState::Local,
            })
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
    fn pending_tags_are_local_refs_not_known_at_the_same_remote_object() {
        let local = parse_refs("object-a\t\t\tv1.0.0\t\t100\nobject-b\t\t\tv1.1.0\t\t110\n");
        let remote = parse_refs("object-a\t\t\tv1.0.0\t\t100\nold-object\t\t\tv1.1.0\t\t90\n");
        let refs = TagRefs::new(local, remote);

        assert_eq!(
            refs.pending()
                .into_iter()
                .map(Tag::name)
                .collect::<Vec<_>>(),
            vec!["v1.1.0"]
        );
    }
}
