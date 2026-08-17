//! Tag parsing helpers for test Git clients.

use std::collections::BTreeMap;

use anyhow::Context as _;
use gtl_models::{
    git::{GitObjectId, TagName},
    tags::Tag,
    timestamps::MachineTimestamp,
};

use super::try_commit_id_fixture;

pub(super) const LOCAL_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)";

pub(super) fn parse_remote_refs(stdout: &str) -> BTreeMap<TagName, GitObjectId> {
    stdout
        .lines()
        .filter_map(|line| {
            let (object, refname) = line.split_once('\t')?;
            let name = refname.strip_prefix("refs/tags/")?;
            if object.is_empty() || name.is_empty() || name.ends_with("^{}") {
                return None;
            }
            Some((
                TagName::try_new(name.to_owned()).ok()?,
                GitObjectId::try_new(object.to_owned()).ok()?,
            ))
        })
        .collect()
}

pub(super) fn parse_refs(stdout: &str) -> anyhow::Result<BTreeMap<TagName, Tag>> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(4, '\t');
            let object = GitObjectId::try_new(fields.next().unwrap_or_default().to_owned())?;
            let peeled_commit = fields.next().unwrap_or_default().to_owned();
            let name = TagName::try_new(fields.next().unwrap_or_default().to_owned())?;
            let message_and_date = fields.next().unwrap_or_default();
            let (message, created_at) = match message_and_date.rsplit_once('\t') {
                Some((message, "")) => (message, None),
                Some((message, raw)) => {
                    let seconds = raw
                        .parse::<i64>()
                        .context("scripted Git tag has an invalid creation epoch")?;
                    let timestamp = MachineTimestamp::from_unix_seconds(seconds)
                        .context("scripted Git tag creation epoch is out of range")?;
                    (message, Some(timestamp))
                }
                None => (message_and_date, None),
            };
            let annotated = !peeled_commit.is_empty();
            let commit = if annotated {
                peeled_commit
            } else {
                object.to_string()
            };
            let commit = try_commit_id_fixture(&commit)?;
            let message = annotated
                .then(|| message.trim().to_string())
                .filter(|message| !message.is_empty());
            let tag = if annotated {
                Tag::annotated(name.clone(), object, commit, created_at, message)
            } else {
                Tag::lightweight(name.clone(), commit, created_at)
            };
            Ok((name, tag))
        })
        .collect()
}
