//! Wire DTOs for the live-views feature.

use serde::{Deserialize, Serialize};

/// A request to validate and persist one live-view source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveLiveViewRequest {
    pub path: String,
}

/// A successfully saved live view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveLiveViewData {
    pub source_kind: String,
    pub source_value: String,
    pub display_name: String,
    pub already_saved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_live_view_request_serializes_path_only() {
        let request = SaveLiveViewRequest {
            path: "/home/user/repo".into(),
        };

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"path": "/home/user/repo"})
        );
    }

    #[test]
    fn save_live_view_request_accepts_legacy_data_root() {
        let legacy: SaveLiveViewRequest = serde_json::from_value(serde_json::json!({
            "data_root": "/redirected",
            "path": "/home/user/repo"
        }))
        .unwrap();

        assert_eq!(legacy.path, "/home/user/repo");
    }

    #[test]
    fn test_save_live_view_data_roundtrip() {
        let original = SaveLiveViewData {
            source_kind: "LocalRepo".to_string(),
            source_value: "/home/user/repo".to_string(),
            display_name: "repo".to_string(),
            already_saved: true,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SaveLiveViewData = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
