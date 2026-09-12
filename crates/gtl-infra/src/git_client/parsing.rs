use std::collections::BTreeMap;

use anyhow::Context as _;
use gtl_models::{
    diffs::CommitId,
    git::{GitObjectId, TagName},
    tags::Tag,
    timestamps::MachineTimestamp,
};

pub(super) fn parse_local_tags(output: &str) -> anyhow::Result<BTreeMap<TagName, Tag>> {
    output
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
                        .context("Git tag has an invalid creation epoch")?;
                    let timestamp = MachineTimestamp::from_unix_seconds(seconds)
                        .context("Git tag creation epoch is out of range")?;
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
            let commit: CommitId = commit.try_into()?;
            let tag = if annotated {
                Tag::annotated(
                    name.clone(),
                    object,
                    commit,
                    created_at,
                    Some(message.trim().to_string()).filter(|message| !message.is_empty()),
                )
            } else {
                Tag::lightweight(name.clone(), commit, created_at)
            };
            Ok((name, tag))
        })
        .collect()
}

pub(super) fn parse_remote_tags(output: &str) -> anyhow::Result<BTreeMap<TagName, GitObjectId>> {
    output
        .lines()
        .filter_map(|line| {
            let (object, reference) = line.split_once('\t')?;
            let name = reference.strip_prefix("refs/tags/")?;
            (!name.ends_with("^{}")).then_some((name, object))
        })
        .map(|(name, object)| {
            Ok((
                TagName::try_new(name.to_owned())?,
                GitObjectId::try_new(object.to_owned())?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_local_tags;

    #[test]
    fn local_tags_reject_an_invalid_resolved_commit_id() {
        assert!(parse_local_tags("invalid\t\tv1.0.0\t\t100").is_err());
    }

    #[test]
    fn local_tags_reject_a_malformed_creation_epoch() {
        let error =
            parse_local_tags("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\tv1.0.0\t\tnot-an-epoch")
                .unwrap_err();

        assert!(error.to_string().contains("invalid creation epoch"));
    }
}
