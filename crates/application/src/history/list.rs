//! The `history/list` vertical slice: the query the desktop history panel
//! dispatches in-process. Sort order (newest-first) is this query's contract,
//! not any one UI's rendering choice.

use std::path::PathBuf;

use cqrsy::Handler;

use crate::ports::{ArtifactStore, HistoryRecord};

#[derive(Debug, Clone, PartialEq, cqrsy::Query)]
#[query(out = ListHistoryResponse, err = ListHistoryError)]
pub struct ListHistory {
    pub store_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListHistoryResponse {
    pub entries: Vec<HistoryRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListHistoryError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ListHistoryHandler<A: ArtifactStore> {
    pub store: A,
}

impl<A: ArtifactStore> Handler<ListHistory> for ListHistoryHandler<A> {
    async fn handle(&self, req: ListHistory) -> Result<ListHistoryResponse, ListHistoryError> {
        // `diff -r`/`diff --all` span multiple repos, so they carry no single
        // head commit — `head_committed_at` is empty. Fall back to `generated_at`
        // for those so they sort by actual recency instead of always trailing.
        fn recency(r: &HistoryRecord) -> &str {
            if r.head_committed_at.is_empty() {
                &r.generated_at
            } else {
                &r.head_committed_at
            }
        }

        let mut entries = self.store.list_history(&req.store_root)?;
        entries.sort_by(|a, b| {
            recency(b)
                .cmp(recency(a))
                .then_with(|| b.generated_at.cmp(&a.generated_at))
        });
        Ok(ListHistoryResponse { entries })
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

    use super::*;
    use crate::{ports::HistoryRecord, testing::InMemoryArtifactStore};

    fn record(repo_id: &str, committed: &str, generated: &str) -> HistoryRecord {
        HistoryRecord {
            repo_id: repo_id.into(),
            repo_name: "n".into(),
            title: "t".into(),
            range_label: "x".into(),
            head_committed_at: committed.into(),
            generated_at: generated.into(),
            content_hash: "h".into(),
            kind: domain::diffs::DiffKind::TwoDot,
            byte_size: 0,
        }
    }

    fn req() -> ListHistory {
        ListHistory {
            store_root: "/store".into(),
        }
    }

    #[test]
    fn sorts_newest_committed_first_then_generated_desc() {
        let store = InMemoryArtifactStore {
            history: vec![
                record("a", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
                record("b", "2026-06-01T00:00:00Z", "2026-06-01T00:00:00Z"),
                record("c", "2026-06-01T00:00:00Z", "2026-06-02T00:00:00Z"),
            ],
            ..Default::default()
        };
        let handler = ListHistoryHandler { store };

        let response = send_now(&(), &handler, req()).expect("list succeeds");

        assert_eq!(response.entries[0].repo_id, "c"); // tie broken by generated desc
        assert_eq!(response.entries[2].repo_id, "a");
    }

    #[test]
    fn falls_back_to_generated_at_when_head_committed_at_is_empty() {
        // `diff -r`/`diff --all` have no single head commit, so they carry an
        // empty `head_committed_at`. Without a fallback they'd always sort last,
        // regardless of how recently they were generated.
        let store = InMemoryArtifactStore {
            history: vec![
                record("old-commit", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
                record("fresh-multi-repo", "", "2026-06-10T00:00:00Z"),
            ],
            ..Default::default()
        };
        let handler = ListHistoryHandler { store };

        let response = send_now(&(), &handler, req()).expect("list succeeds");

        assert_eq!(response.entries[0].repo_id, "fresh-multi-repo");
        assert_eq!(response.entries[1].repo_id, "old-commit");
    }

    #[test]
    fn empty_store_returns_an_empty_list() {
        let handler = ListHistoryHandler {
            store: InMemoryArtifactStore::default(),
        };

        let response = send_now(&(), &handler, req()).expect("list succeeds");

        assert!(response.entries.is_empty());
    }
}
