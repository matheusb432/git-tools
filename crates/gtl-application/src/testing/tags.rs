use std::collections::BTreeMap;

use gtl_models::tags::Tag;

pub(super) const LOCAL_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)";

pub(super) fn parse_remote_refs(stdout: &str) -> BTreeMap<String, String> {
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
