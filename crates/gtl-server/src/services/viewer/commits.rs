use gtl_models::diffs::Commit;
use gtl_wire::{
    v1,
    viewer::{
        VIEWER_COMMIT_BODY_MAX_BYTES, VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES,
        VIEWER_COMMIT_PAGE_MAX_ENTRIES,
    },
};
use prost::Message as _;
use tonic::Status;

pub(super) fn page(
    proto_identity: v1::ViewerViewIdentity,
    commits: &[Commit],
    cursor: Option<u32>,
) -> Result<v1::ListViewerCommitsResponse, Status> {
    let cursor = usize::try_from(cursor.unwrap_or_default())
        .map_err(|_| Status::invalid_argument("cursor is invalid"))?;
    if cursor > commits.len() {
        return Err(Status::invalid_argument(
            "cursor is outside the commit list",
        ));
    }
    let mut response = v1::ListViewerCommitsResponse {
        identity: Some(proto_identity),
        commits: Vec::new(),
        next_cursor: None,
    };
    // Reserve the tag and largest u32 varint for the next cursor.
    let mut encoded_bytes = response.encoded_len() + 6;
    let mut index = cursor;
    while index < commits.len() && response.commits.len() < VIEWER_COMMIT_PAGE_MAX_ENTRIES {
        let commit = &commits[index];
        let mut projected =
            commit_summary(commit, commit.body.len() > VIEWER_COMMIT_BODY_MAX_BYTES);
        let mut entry_bytes = commit_entry_bytes(&projected);
        if encoded_bytes + entry_bytes > VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES {
            if !response.commits.is_empty() {
                break;
            }
            projected.body.clear();
            projected.body_omitted = true;
            entry_bytes = commit_entry_bytes(&projected);
            if encoded_bytes + entry_bytes > VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES {
                return Err(Status::resource_exhausted(
                    "one commit summary exceeds the viewer page limit",
                ));
            }
        }
        encoded_bytes += entry_bytes;
        response.commits.push(projected);
        index += 1;
    }
    response.next_cursor = (index < commits.len())
        .then(|| u32::try_from(index))
        .transpose()
        .map_err(|_| Status::resource_exhausted("commit cursor exceeds u32"))?;
    Ok(response)
}

fn commit_entry_bytes(commit: &v1::ViewerCommitSummary) -> usize {
    let bytes = commit.encoded_len();
    1 + prost::length_delimiter_len(bytes) + bytes
}

fn commit_summary(
    commit: &gtl_models::diffs::Commit,
    body_omitted: bool,
) -> v1::ViewerCommitSummary {
    v1::ViewerCommitSummary {
        id: commit.id.to_string(),
        subject: commit.subject.clone(),
        body: if body_omitted {
            String::new()
        } else {
            commit.body.clone()
        },
        committed_at: commit.committed_at.as_ref().to_owned(),
        is_merge: commit.is_merge(),
        body_omitted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(body: String) -> Commit {
        Commit {
            id: "a".repeat(40).try_into().unwrap(),
            subject: "subject".into(),
            body,
            committed_at: "2026-09-08T00:00:00Z".try_into().unwrap(),
            parents: Vec::new(),
        }
    }

    #[test]
    fn page_bound_includes_cursor_and_preserves_entries_across_pages() {
        let commits = vec![
            commit("x".repeat(VIEWER_COMMIT_BODY_MAX_BYTES));
            VIEWER_COMMIT_PAGE_MAX_ENTRIES + 1
        ];
        let mut cursor = None;
        let mut count = 0;
        loop {
            let response = page(v1::ViewerViewIdentity::default(), &commits, cursor).unwrap();
            assert!(response.encoded_len() <= VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES);
            assert!(response.commits.len() <= VIEWER_COMMIT_PAGE_MAX_ENTRIES);
            assert!(!response.commits.is_empty());
            count += response.commits.len();
            cursor = response.next_cursor;
            if cursor.is_none() {
                break;
            }
            assert_eq!(usize::try_from(cursor.unwrap()).unwrap(), count);
        }
        assert_eq!(count, commits.len());
    }

    #[test]
    fn oversized_body_is_omitted_and_invalid_cursor_is_rejected() {
        let commits = [commit("x".repeat(VIEWER_COMMIT_BODY_MAX_BYTES + 1))];
        let response = page(v1::ViewerViewIdentity::default(), &commits, None).unwrap();
        assert!(response.commits[0].body_omitted);
        assert!(response.commits[0].body.is_empty());
        assert!(response.next_cursor.is_none());
        assert_eq!(
            page(v1::ViewerViewIdentity::default(), &commits, Some(2))
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
}
