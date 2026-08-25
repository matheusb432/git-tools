//! The `history/list` vertical slice: the query the desktop history panel
//! dispatches in-process. Sort order (newest-first) is this query's contract,
//! not any one UI's rendering choice.

use std::{cmp::Ordering, path::PathBuf};

use crate::ports::{ArtifactStore, HistoryRecord};

#[derive(Debug, thiserror::Error)]
pub enum ListHistoryError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

// Multi-repo renders have no single head commit, so generation time is their recency.
fn compare_recency(left: &HistoryRecord, right: &HistoryRecord) -> Ordering {
    match (&left.head_committed_at, &right.head_committed_at) {
        (Some(left), Some(right)) => left.cmp(right),
        (None, None) => left.generated_at.cmp(&right.generated_at),
        (Some(left), None) => left.cmp(&right.generated_at),
        (None, Some(right)) => left.generated_at.cmp(right),
    }
}

/// Lists artifact history in newest-first order.
#[cqrsy::query]
#[expect(
    clippy::needless_pass_by_value,
    reason = "CQRsy operations own their request value"
)]
pub fn execute(
    store_root: PathBuf,
    store: &impl ArtifactStore,
) -> Result<Vec<HistoryRecord>, ListHistoryError> {
    let mut entries = store.list_history(&store_root)?;
    entries.sort_by(|a, b| compare_recency(b, a).then_with(|| b.generated_at.cmp(&a.generated_at)));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        artifacts::{ArtifactByteSize, ArtifactContentHash, RepositoryStoreId},
        timestamps::MachineTimestamp,
    };

    use super::*;
    use crate::{history::list_history, ports::HistoryRecord, utils::InMemoryArtifactStore};

    fn record(repo_id: &str, committed: &str, generated: &str) -> HistoryRecord {
        HistoryRecord {
            repo_id: RepositoryStoreId::try_new(repo_id.to_owned())
                .expect("fixture repository store ID is valid"),
            repo_name: crate::utils::project_name("n"),
            title: "t".into(),
            range_label: "x".into(),
            head_committed_at: (!committed.is_empty()).then(|| {
                MachineTimestamp::try_from(committed).expect("fixture commit timestamp is valid")
            }),
            generated_at: MachineTimestamp::try_from(generated)
                .expect("fixture generation timestamp is valid"),
            content_hash: ArtifactContentHash::try_new("dddddddddddddddd".to_owned())
                .expect("fixture content hash is valid"),
            kind: gtl_models::diffs::DiffKind::TwoDot,
            byte_size: ArtifactByteSize::default(),
        }
    }

    fn req() -> PathBuf {
        "/store".into()
    }

    #[test]
    fn sorts_newest_committed_first_then_generated_desc() {
        let store = InMemoryArtifactStore {
            history: vec![
                record(
                    "aaaaaaaaaaaaaaaa",
                    "2026-01-01T00:00:00Z",
                    "2026-01-01T00:00:00Z",
                ),
                record(
                    "bbbbbbbbbbbbbbbb",
                    "2026-06-01T00:00:00Z",
                    "2026-06-01T00:00:00Z",
                ),
                record(
                    "cccccccccccccccc",
                    "2026-06-01T00:00:00Z",
                    "2026-06-02T00:00:00Z",
                ),
            ],
            ..Default::default()
        };
        let response = list_history::execute(req(), &store).expect("list succeeds");

        assert_eq!(response[0].repo_id.as_ref(), "cccccccccccccccc");
        assert_eq!(response[2].repo_id.as_ref(), "aaaaaaaaaaaaaaaa");
    }

    #[test]
    fn falls_back_to_generated_at_when_head_commit_is_absent() {
        // `diff -r`/`diff --all` have no single head commit. Without a fallback
        // they would always sort last,
        // regardless of how recently they were generated.
        let store = InMemoryArtifactStore {
            history: vec![
                record(
                    "1111111111111111",
                    "2026-01-01T00:00:00Z",
                    "2026-01-01T00:00:00Z",
                ),
                record("ffffffffffffffff", "", "2026-06-10T00:00:00Z"),
            ],
            ..Default::default()
        };
        let response = list_history::execute(req(), &store).expect("list succeeds");

        assert_eq!(response[0].repo_id.as_ref(), "ffffffffffffffff");
        assert_eq!(response[1].repo_id.as_ref(), "1111111111111111");
    }

    #[test]
    fn recency_order_uses_instants_instead_of_lexical_offsets() {
        let store = InMemoryArtifactStore {
            history: vec![
                record(
                    "1111111111111111",
                    "2026-01-01T00:30:00+01:00",
                    "2026-01-01T00:30:00+01:00",
                ),
                record(
                    "2222222222222222",
                    "2025-12-31T23:45:00Z",
                    "2025-12-31T23:45:00Z",
                ),
            ],
            ..Default::default()
        };

        let response = list_history::execute(req(), &store).expect("list succeeds");

        assert_eq!(response[0].repo_id.as_ref(), "2222222222222222");
    }

    #[test]
    fn empty_store_returns_an_empty_list() {
        let store = InMemoryArtifactStore::default();
        let response = list_history::execute(req(), &store).expect("list succeeds");

        assert!(response.is_empty());
    }
}
