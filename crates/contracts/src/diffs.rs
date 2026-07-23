//! Diff response values shared across process boundaries.

use serde::{Deserialize, Serialize};

/// A successful render result: the artifact path and whether it was reused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffData {
    pub artifact: String,
    pub reused: bool,
}
