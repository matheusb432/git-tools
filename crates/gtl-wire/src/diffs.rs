//! Diff response values shared across process boundaries.

use gtl_models::paths::AbsoluteFilePath;
use serde::{Deserialize, Serialize};

/// A successful render result with its closed placement origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "placement", rename_all = "snake_case")]
pub enum RenderDiffData {
    /// The render created a new artifact.
    Created { artifact: AbsoluteFilePath },
    /// The render reused an identical artifact.
    Reused { artifact: AbsoluteFilePath },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::paths::AbsoluteFilePath;

    use super::RenderDiffData;

    #[test]
    fn render_placement_json_is_explicit() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(RenderDiffData::Created {
                artifact: AbsoluteFilePath::try_new(PathBuf::from("/tmp/diff.html"))
                    .expect("absolute fixture path"),
            })?,
            serde_json::json!({
                "placement": "created",
                "artifact": "/tmp/diff.html",
            })
        );
        assert_eq!(
            serde_json::to_value(RenderDiffData::Reused {
                artifact: AbsoluteFilePath::try_new(PathBuf::from("/tmp/diff.html"))
                    .expect("absolute fixture path"),
            })?,
            serde_json::json!({
                "placement": "reused",
                "artifact": "/tmp/diff.html",
            })
        );
        Ok(())
    }
}
