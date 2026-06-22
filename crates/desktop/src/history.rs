//! History view DTO: maps store sidecars to frontend rows, newest-first.
use gtl_store::Sidecar;
use serde::Serialize;
use std::path::Path;

/// One history row for the frontend. `url` is the tab's `diff://` source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryEntry {
    pub repo_id: String,
    pub repo_name: String,
    pub title: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub content_hash: String,
    pub url: String,
}

pub(crate) fn to_entry(content_hash: String, s: Sidecar) -> HistoryEntry {
    let url = format!("diff://{}/{}", s.repo_id, content_hash);
    HistoryEntry {
        repo_id: s.repo_id,
        repo_name: s.repo_name,
        title: s.title,
        range_label: s.range_label,
        head_committed_at: s.head_committed_at,
        generated_at: s.generated_at,
        content_hash,
        url,
    }
}

/// Newest-first by `head_committed_at`, breaking ties on `generated_at` descending.
pub fn sort_entries(mut entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    entries.sort_by(|a, b| {
        b.head_committed_at
            .cmp(&a.head_committed_at)
            .then_with(|| b.generated_at.cmp(&a.generated_at))
    });
    entries
}

/// Read the store and return sorted history rows. Errors collapse to empty.
pub fn entries_from_store(store_root: &Path) -> Vec<HistoryEntry> {
    let paired = gtl_store::list_history_with_hash(store_root).unwrap_or_default();
    sort_entries(paired.into_iter().map(|(h, s)| to_entry(h, s)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(committed: &str, generated: &str) -> HistoryEntry {
        HistoryEntry {
            repo_id: "r".into(),
            repo_name: "n".into(),
            title: "t".into(),
            range_label: "x".into(),
            head_committed_at: committed.into(),
            generated_at: generated.into(),
            content_hash: "h".into(),
            url: "diff://r/h".into(),
        }
    }

    #[test]
    fn sorts_newest_committed_first_then_generated() {
        let got = sort_entries(vec![
            entry("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
            entry("2026-06-01T00:00:00Z", "2026-06-01T00:00:00Z"),
            entry("2026-06-01T00:00:00Z", "2026-06-02T00:00:00Z"),
        ]);
        assert_eq!(got[0].generated_at, "2026-06-02T00:00:00Z"); // tie broken by generated desc
        assert_eq!(got[2].head_committed_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn url_is_built_from_repo_id_and_hash() {
        let e = to_entry(
            "fedcba9876543210".into(),
            gtl_store::Sidecar {
                repo_id: "0123456789abcdef".into(),
                repo_name: "n".into(),
                repo_root: "/r".into(),
                kind: gtl_store::DiffKind::TwoDot,
                base_sha: "a".into(),
                head_sha: "b".into(),
                range_label: "x".into(),
                head_committed_at: "t".into(),
                generated_at: "t".into(),
                title: "t".into(),
                byte_size: 0,
            },
        );
        assert_eq!(e.url, "diff://0123456789abcdef/fedcba9876543210");
    }
}
