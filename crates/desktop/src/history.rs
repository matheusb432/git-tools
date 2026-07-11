//! Maps persisted history records to the legacy shell and htmx viewer models.
use application::ports::{HistoryRecord, RecentRenderRecord};
use domain::{diffs::DiffKind, viewer::ViewerHistoryEntry};
use serde::Serialize;

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
    pub kind: String,
    pub byte_size: u64,
    pub url: String,
}

/// Stable short tag for a `DiffKind`, for the history-row badge.
fn kind_tag(kind: DiffKind) -> String {
    match kind {
        DiffKind::TwoDot => "2-dot",
        DiffKind::ThreeDot => "3-dot",
        DiffKind::WorkTree => "worktree",
    }
    .to_string()
}

pub(crate) fn to_entry(r: HistoryRecord) -> HistoryEntry {
    let url = format!("diff://{}/{}", r.repo_id, r.content_hash);
    let kind = kind_tag(r.kind);
    HistoryEntry {
        repo_id: r.repo_id,
        repo_name: r.repo_name,
        title: r.title,
        range_label: r.range_label,
        head_committed_at: r.head_committed_at,
        generated_at: r.generated_at,
        content_hash: r.content_hash,
        kind,
        byte_size: r.byte_size,
        url,
    }
}

pub(crate) fn to_viewer_entry(record: RecentRenderRecord) -> ViewerHistoryEntry {
    ViewerHistoryEntry::new(
        record.id,
        record.title,
        record.repo_name,
        record.kind,
        record.range_label,
        record.rendered_at,
    )
}

#[cfg(test)]
mod tests {
    use application::ports::HistoryRecord;
    use domain::diffs::DiffKind;

    use super::*;

    fn record(kind: DiffKind, byte_size: u64) -> HistoryRecord {
        HistoryRecord {
            repo_id: "0123456789abcdef".into(),
            repo_name: "n".into(),
            title: "t".into(),
            range_label: "main...HEAD".into(),
            head_committed_at: "t".into(),
            generated_at: "t".into(),
            content_hash: "fedcba9876543210".into(),
            kind,
            byte_size,
        }
    }

    #[test]
    fn url_is_built_from_repo_id_and_hash() {
        let e = to_entry(record(DiffKind::TwoDot, 0));
        assert_eq!(e.url, "diff://0123456789abcdef/fedcba9876543210");
    }

    #[test]
    fn entry_exposes_kind_tag_and_byte_size() {
        let e = to_entry(record(DiffKind::ThreeDot, 4096));
        assert_eq!(e.kind, "3-dot");
        assert_eq!(e.byte_size, 4096);
    }
}
