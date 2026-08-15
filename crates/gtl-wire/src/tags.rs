//! Wire DTOs for the stateless tag-bump dry-run and mutation protocol.

use gtl_models::diffs::CommitId;
use serde::{Deserialize, Serialize};

/// The `SemVer` component advanced by a tag bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagBumpLevelDto {
    Major,
    Minor,
    Patch,
}

/// Requests an exact tag-bump proposal without mutating Git state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunTagBumpRequest {
    pub repo_path: String,
    pub level: TagBumpLevelDto,
    pub message: String,
    pub push: bool,
}

/// The exact mutation displayed before the user confirms a tag bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagBumpPreview {
    pub repo_path: String,
    pub branch: String,
    pub target_id: CommitId,
    pub level: TagBumpLevelDto,
    pub base_tag: String,
    pub next_tag: String,
    pub message: String,
    pub push: bool,
}

/// Applies the exact preview displayed by the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumpTagRequest {
    pub preview: TagBumpPreview,
}

/// The closed application status of a tag bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagBumpStatusDto {
    Created,
    Noop,
    Pushed,
    Failed,
}

/// The result of attempting the exact displayed tag mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumpTagData {
    pub tag: String,
    pub status: TagBumpStatusDto,
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{BumpTagRequest, DryRunTagBumpRequest, TagBumpLevelDto, TagBumpPreview};
    use crate::testing::commit_id;

    fn preview() -> TagBumpPreview {
        TagBumpPreview {
            repo_path: "/repo".into(),
            branch: "main".into(),
            target_id: commit_id("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            level: TagBumpLevelDto::Minor,
            base_tag: "v0.30.0".into(),
            next_tag: "v0.31.0".into(),
            message: "release".into(),
            push: true,
        }
    }

    #[test]
    fn dry_run_request_serializes_the_repository_and_mutation_intent() {
        assert_eq!(
            serde_json::to_value(DryRunTagBumpRequest {
                repo_path: "/repo".into(),
                level: TagBumpLevelDto::Patch,
                message: "release".into(),
                push: false,
            })
            .unwrap(),
            json!({
                "repo_path": "/repo",
                "level": "patch",
                "message": "release",
                "push": false
            })
        );
    }

    #[test]
    fn bump_request_carries_the_complete_displayed_preview() {
        let request = BumpTagRequest { preview: preview() };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["preview"]["next_tag"], "v0.31.0");
        assert_eq!(
            serde_json::from_value::<BumpTagRequest>(value).unwrap(),
            request
        );
    }

    #[test]
    fn preview_wire_shape_has_no_daemon_owned_identity_or_expiration_fields() {
        let value = serde_json::to_value(preview()).unwrap();

        let object = value.as_object().expect("preview object");
        let mut fields = object.keys().map(String::as_str).collect::<Vec<_>>();
        fields.sort_unstable();
        assert_eq!(
            fields,
            vec![
                "base_tag",
                "branch",
                "level",
                "message",
                "next_tag",
                "push",
                "repo_path",
                "target_id",
            ]
        );
    }
}
