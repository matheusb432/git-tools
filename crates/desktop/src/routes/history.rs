use application::{history::RecentRenderRecord, viewer::ViewerHistoryEntry};

pub(super) fn to_viewer_entry(record: RecentRenderRecord) -> ViewerHistoryEntry {
    ViewerHistoryEntry::new(
        record.id,
        record.title,
        record.repo_name,
        record.kind,
        record.range_label,
        record.rendered_at,
        record.recipe_json,
    )
}
