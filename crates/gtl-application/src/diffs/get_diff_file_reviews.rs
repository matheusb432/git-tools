use gtl_wire::diff_review::DiffFileReviewReference;
use rusqlite::{Connection, params};

#[cqrsy::query]
pub fn execute(
    request: &[DiffFileReviewReference],
    connection: &Connection,
) -> anyhow::Result<HashSet<DiffFileReviewReference>> {
    let mut statement = connection.prepare_cached(
        "SELECT EXISTS(SELECT 1 FROM diff_file_reviews
         WHERE repository_root = ?1 AND file_path = ?2 AND content_id = ?3)",
    )?;
    let mut reviewed = HashSet::new();
    for file in request {
        let digest = file.content_id.into_digest();
        let marked: bool = statement.query_row(
            params![
                file.repository.to_str(),
                file.path.to_str(),
                digest.as_slice()
            ],
            |row| row.get(0),
        )?;
        if marked {
            reviewed.insert(file.clone());
        }
    }
    Ok(reviewed)
}
use std::collections::HashSet;
