use gtl_models::diffs::{DiffReviewContentId, DiffReviewScope};
use gtl_wire::diff_review::{DiffFileReview, DiffFileReviewReference};
use sha2::{Digest as _, Sha256};

use super::FileDiff;

pub(crate) fn file_review(scope: &DiffReviewScope, file: &FileDiff) -> DiffFileReview {
    let mut digest = Sha256::new();
    digest.update(b"gtl.diff.review-content.v1\0");
    let path = file.path.as_path().as_os_str().as_encoded_bytes();
    digest.update((path.len() as u64).to_be_bytes());
    digest.update(path);
    digest.update(file.lines.fingerprint());
    DiffFileReview {
        reference: DiffFileReviewReference {
            scope: scope.clone(),
            path: file.path.clone(),
            content_id: DiffReviewContentId::from_digest(digest.finalize().into()),
        },
        reviewed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_identity_tracks_compared_source_and_path_but_ignores_full_context()
    -> anyhow::Result<()> {
        let mut view = crate::utils::diffs::view();
        view.files = super::super::unified_diff::parse(crate::utils::diffs::DIFF_SINGLE_FILE)?;
        let scope = view.origin.review_scope();
        let original = file_review(&scope, &view.files[0]);
        view.files[0].full_lines = Some(["different context"].into_iter().collect());
        assert_eq!(original, file_review(&scope, &view.files[0]));
        view.files[0].lines = ["+edited"].into_iter().collect();
        assert_ne!(original, file_review(&scope, &view.files[0]));
        let edited = file_review(&scope, &view.files[0]);
        view.files[0].path = crate::utils::repository_relative_path("renamed.rs");
        assert_ne!(edited, file_review(&scope, &view.files[0]));
        Ok(())
    }
}
