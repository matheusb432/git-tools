use gtl_models::{diffs::DiffReviewScope, failure::ErrorMeta};
use gtl_wire::diff_review::SetDiffFileReviewed;
use rusqlite::{Connection, params};

const REVIEW_VERSIONS_PER_FILE_MAX: i64 = 16;

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SetDiffFileReviewedError {
    #[error(transparent)]
    #[meta(private(Internal))]
    Persistence(#[from] rusqlite::Error),
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] crate::viewer::ViewerStateError),
}

#[cqrsy::command]
pub fn execute(
    request: &SetDiffFileReviewed,
    connection: &mut Connection,
    state: &crate::viewer::ViewerState,
) -> Result<(), SetDiffFileReviewedError> {
    let transaction = connection.transaction()?;
    let digest = request.file.content_id.into_digest();
    let path = request.file.path.to_str();
    match (&request.file.scope, request.reviewed) {
        (DiffReviewScope::Repository(repository), true) => {
            let repository = repository.to_str();
            transaction.execute(
                "INSERT INTO diff_file_reviews (repository_root, file_path, content_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(repository_root, file_path, content_id) DO NOTHING",
                params![repository, path, digest.as_slice()],
            )?;
            transaction.execute(
                "DELETE FROM diff_file_reviews
                 WHERE repository_root = ?1 AND file_path = ?2 AND id NOT IN
                 (SELECT id FROM diff_file_reviews WHERE repository_root = ?1 AND file_path = ?2
                  ORDER BY id DESC LIMIT ?3)",
                params![repository, path, REVIEW_VERSIONS_PER_FILE_MAX],
            )?;
        }
        (DiffReviewScope::Repository(repository), false) => {
            transaction.execute(
                "DELETE FROM diff_file_reviews
                 WHERE repository_root = ?1 AND file_path = ?2 AND content_id = ?3",
                params![repository.to_str(), path, digest.as_slice()],
            )?;
        }
        (DiffReviewScope::Text, true) => {
            transaction.execute(
                "INSERT INTO diff_text_file_reviews (file_path, content_id) VALUES (?1, ?2)
                 ON CONFLICT(file_path, content_id) DO NOTHING",
                params![path, digest.as_slice()],
            )?;
            transaction.execute(
                "DELETE FROM diff_text_file_reviews
                 WHERE file_path = ?1 AND id NOT IN
                 (SELECT id FROM diff_text_file_reviews WHERE file_path = ?1
                  ORDER BY id DESC LIMIT ?2)",
                params![path, REVIEW_VERSIONS_PER_FILE_MAX],
            )?;
        }
        (DiffReviewScope::Text, false) => {
            transaction.execute(
                "DELETE FROM diff_text_file_reviews WHERE file_path = ?1 AND content_id = ?2",
                params![path, digest.as_slice()],
            )?;
        }
    }
    transaction.commit()?;
    state.mark_shell_changed()?;
    Ok(())
}
