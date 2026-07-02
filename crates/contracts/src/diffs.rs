//! Wire DTOs for the diffs feature.

use serde::{Deserialize, Serialize};

/// The wire form of a diff target selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffTargetDto {
    Unpushed,
    Base { rev: String },
    Range { range: String },
    Merge { base: String },
    Last { count: u32 },
}

/// A request to render a diff preview artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffRequest {
    pub cwd: String,
    pub store_root: String,
    pub target: DiffTargetDto,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
}

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
    fn test_diff_target_unpushed_wire_format() {
        let target = DiffTargetDto::Unpushed;
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"kind\":\"unpushed\""));
        let deserialized: DiffTargetDto = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_diff_target_base_wire_format() {
        let target = DiffTargetDto::Base {
            rev: "abc123".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"kind\":\"base\""));
        let deserialized: DiffTargetDto = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_diff_target_range_wire_format() {
        let target = DiffTargetDto::Range {
            range: "main..head".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"kind\":\"range\""));
        let deserialized: DiffTargetDto = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_diff_target_merge_wire_format() {
        let target = DiffTargetDto::Merge {
            base: "main".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"kind\":\"merge\""));
        let deserialized: DiffTargetDto = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_diff_target_last_wire_format() {
        let target = DiffTargetDto::Last { count: 3 };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"kind\":\"last\""));
        let deserialized: DiffTargetDto = serde_json::from_str(&json).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_render_diff_request_roundtrip() {
        let original = RenderDiffRequest {
            cwd: "/home/user/repo".to_string(),
            store_root: "/home/user/.local/share/git-tools".to_string(),
            target: DiffTargetDto::Base {
                rev: "main".to_string(),
            },
            name: Some("my-diff".to_string()),
            theme: Some("dark".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RenderDiffRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_render_diff_request_without_optional_fields() {
        let original = RenderDiffRequest {
            cwd: "/home/user/repo".to_string(),
            store_root: "/home/user/.local/share/git-tools".to_string(),
            target: DiffTargetDto::Unpushed,
            name: None,
            theme: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RenderDiffRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_render_diff_data_roundtrip() {
        let original = RenderDiffData {
            artifact: "/path/to/artifact.html".to_string(),
            reused: true,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RenderDiffData = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
