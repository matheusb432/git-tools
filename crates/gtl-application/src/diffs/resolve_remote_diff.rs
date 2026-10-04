use anyhow::Context as _;
use gtl_models::{
    diffs::DiffTextId,
    failure::{ErrorMeta, RemoteDiffFailure},
};
use rusqlite::{Connection, OptionalExtension as _, params};

use super::remote_diff::{GitHubApiRequest, RemoteDiffKey, StoredRemoteDiff};

/// Raw caller input naming a hosted repository and the revisions to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveRemoteDiff {
    pub origin: String,
    pub range: String,
    /// Fetch again even when a cached copy exists.
    pub refresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDiffResolution {
    Stored(StoredRemoteDiff),
    /// The caller sends this request once and stores GitHub's answer with `store_remote_diff`.
    Fetch(GitHubApiRequest),
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ResolveRemoteDiffError {
    #[error(transparent)]
    #[meta(failure)]
    Remote(#[from] RemoteDiffFailure),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Answers with the cached diff, marking it most recently used, or with the GitHub request that
/// fetches it when no copy exists or `refresh` is set.
#[cqrsy::command]
pub fn execute(
    request: &ResolveRemoteDiff,
    connection: &mut Connection,
) -> Result<RemoteDiffResolution, ResolveRemoteDiffError> {
    let key = RemoteDiffKey::parse(&request.origin, &request.range)?;
    if !request.refresh
        && let Some(id) = use_cached_text_id(&key, connection)?
    {
        return Ok(RemoteDiffResolution::Stored(StoredRemoteDiff {
            id,
            label: key.label(),
        }));
    }
    Ok(RemoteDiffResolution::Fetch(key.compare_request()))
}

fn use_cached_text_id(
    key: &RemoteDiffKey,
    connection: &Connection,
) -> anyhow::Result<Option<DiffTextId>> {
    connection
        .query_row(
            "UPDATE github_compare_diffs
             SET used_order = (SELECT max(used_order) + 1 FROM github_compare_diffs)
             WHERE repository = ?1 AND base = ?2 AND head = ?3
               AND text_id IN (SELECT text_id FROM diff_texts)
             RETURNING text_id",
            params![
                key.repository.to_string(),
                key.range.base.as_str(),
                key.range.head.as_str(),
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|text_id| {
            DiffTextId::try_new(text_id).context("cached remote diff has an invalid text ID")
        })
        .transpose()
}
