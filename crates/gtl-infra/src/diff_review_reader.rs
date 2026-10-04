use std::collections::HashSet;

use gtl_application::{diffs::get_diff_file_reviews, ports::DiffReviewReader};
use gtl_models::diffs::DiffFileReviewReference;

use crate::app_state::SqliteAppState;

impl DiffReviewReader for SqliteAppState {
    fn reviewed_files(
        &self,
        files: &[DiffFileReviewReference],
    ) -> anyhow::Result<HashSet<DiffFileReviewReference>> {
        get_diff_file_reviews::execute(files, &*self.connection_lock()?)
    }
}
