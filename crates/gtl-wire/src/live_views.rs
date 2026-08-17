//! Wire DTOs for the live-views feature.

use gtl_models::{live_views::LiveSource, paths::ProjectName};
use serde::{Deserialize, Serialize};

/// A request to validate and persist one live-view source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveLiveViewRequest {
    pub path: String,
}

/// A successfully saved live view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveLiveViewData {
    #[serde(flatten)]
    pub source: LiveSource,
    pub display_name: ProjectName,
    pub disposition: SaveLiveViewDisposition,
}

/// Distinguishes a first save from refreshing an existing saved source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveLiveViewDisposition {
    Created,
    Refreshed,
}

#[cfg(test)]
mod tests {
    use gtl_models::paths::RepositoryRoot;

    use super::*;

    fn root(value: &str) -> RepositoryRoot {
        RepositoryRoot::try_new(value.into()).expect("absolute repository root")
    }

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
    fn saved_data_preserves_typed_source_and_disposition_in_json() {
        let data = SaveLiveViewData {
            source: LiveSource::local_repo(root("/home/user/repo")),
            display_name: ProjectName::try_new("repo").expect("project name"),
            disposition: SaveLiveViewDisposition::Refreshed,
        };

        assert_eq!(
            serde_json::to_value(data).unwrap(),
            serde_json::json!({
                "source_kind": "LocalRepo",
                "source_value": "/home/user/repo",
                "display_name": "repo",
                "disposition": "refreshed"
            })
        );
    }
}
