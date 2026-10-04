use gtl_models::failure::ErrorMeta;
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
    let values = params![
        request.file.repository.to_str(),
        request.file.path.to_str(),
        digest.as_slice(),
    ];
    if request.reviewed {
        transaction.execute(
            "INSERT INTO diff_file_reviews (repository_root, file_path, content_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(repository_root, file_path, content_id) DO NOTHING",
            values,
        )?;
        transaction.execute(
            "DELETE FROM diff_file_reviews
             WHERE repository_root = ?1 AND file_path = ?2 AND id NOT IN
             (SELECT id FROM diff_file_reviews WHERE repository_root = ?1 AND file_path = ?2
              ORDER BY id DESC LIMIT ?3)",
            params![
                request.file.repository.to_str(),
                request.file.path.to_str(),
                REVIEW_VERSIONS_PER_FILE_MAX
            ],
        )?;
    } else {
        transaction.execute(
            "DELETE FROM diff_file_reviews
             WHERE repository_root = ?1 AND file_path = ?2 AND content_id = ?3",
            values,
        )?;
    }
    transaction.commit()?;
    state.mark_shell_changed()?;
    Ok(())
}
