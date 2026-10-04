use gtl_models::{
    diffs::DiffText,
    failure::{DiffTextFailure, ErrorMeta},
};
use rusqlite::{Connection, params};

/// Unreferenced texts stay available to recent terminal and artifact renders up to this count.
const UNREFERENCED_DIFF_TEXTS_MAX: i64 = 16;

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum StoreDiffTextError {
    #[error(transparent)]
    #[meta(failure)]
    Invalid(#[from] DiffTextFailure),
    #[error(transparent)]
    #[meta(private(Internal))]
    Persistence(#[from] rusqlite::Error),
}

/// Validates and stores `text`, then prunes the oldest unreferenced texts.
#[cqrsy::command]
pub fn execute(text: &DiffText, connection: &mut Connection) -> Result<(), StoreDiffTextError> {
    super::text_diff::parse_files(text)?;
    let transaction = connection.transaction()?;
    insert_and_prune(text, &transaction)?;
    transaction.commit()?;
    Ok(())
}

/// Inserts `text`, then prunes the oldest texts no history entry, tab, or cached remote diff
/// references.
pub(super) fn insert_and_prune(text: &DiffText, connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO diff_texts (text_id, content) VALUES (?1, ?2)",
        params![text.id().as_ref(), text.as_str()],
    )?;
    connection.execute(
        "DELETE FROM diff_texts WHERE id IN (
             SELECT id FROM diff_texts
             WHERE text_id NOT IN (
                 SELECT render_sources.value FROM render_sources
                 JOIN recent_renders ON recent_renders.source_id = render_sources.id
                 WHERE render_sources.kind = 'text')
               AND text_id NOT IN (
                 SELECT json_extract(recipe_json, '$.source.value.id') FROM viewer_tabs
                 WHERE json_extract(recipe_json, '$.source.kind') = 'text')
               AND text_id NOT IN (SELECT text_id FROM github_compare_diffs)
             ORDER BY id DESC
             LIMIT -1 OFFSET ?1)",
        params![UNREFERENCED_DIFF_TEXTS_MAX],
    )?;
    Ok(())
}
