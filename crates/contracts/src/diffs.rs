//! Diff response values shared across process boundaries.

use serde::{Deserialize, Serialize};

/// A successful render result: the artifact path and whether it was reused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffData {
    pub artifact: String,
    pub reused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_diff_data_roundtrips() {
        let original = RenderDiffData {
            artifact: "/path/to/artifact.html".to_string(),
            reused: true,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RenderDiffData = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
