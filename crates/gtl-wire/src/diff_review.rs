pub use gtl_models::diffs::DiffFileReviewReference;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFileReview {
    pub reference: DiffFileReviewReference,
    pub reviewed: bool,
}

/// Sets the mark for this exact compared content; retries are idempotent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetDiffFileReviewed {
    pub file: DiffFileReviewReference,
    pub reviewed: bool,
}
