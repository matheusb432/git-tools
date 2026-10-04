use std::collections::HashSet;

use gtl_models::diffs::DiffFileReviewReference;

/// Reads durable review marks for the exact file versions a frontend displays.
pub trait DiffReviewReader {
    fn reviewed_files(
        &self,
        files: &[DiffFileReviewReference],
    ) -> anyhow::Result<HashSet<DiffFileReviewReference>>;
}

#[cfg(test)]
impl<S: std::hash::BuildHasher> DiffReviewReader for HashSet<DiffFileReviewReference, S> {
    fn reviewed_files(
        &self,
        files: &[DiffFileReviewReference],
    ) -> anyhow::Result<HashSet<DiffFileReviewReference>> {
        Ok(files
            .iter()
            .filter(|file| self.contains(*file))
            .cloned()
            .collect())
    }
}
