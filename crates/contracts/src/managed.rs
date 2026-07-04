//! Wire DTOs for the managed feature's daemon endpoints (`push-all`/`pull-all`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushAllRequest {
    pub repos_file: String,
    pub home_dir: String,
    pub dry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullAllRequest {
    pub repos_file: String,
    pub home_dir: String,
    pub dry: bool,
}

/// The exact JSON shape the CLI's `--json` mode has always produced
/// (`PushPullResult`'s retired `#[serde(rename_all = "PascalCase")]`), preserved
/// so `--json` output stays byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RepoSyncResultDto {
    pub name: String,
    pub branch: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncExitDto {
    Clean,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncData {
    pub results: Vec<RepoSyncResultDto>,
    pub exit: SyncExitDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_all_request_roundtrips() {
        let original = PushAllRequest {
            repos_file: "/repos.toml".into(),
            home_dir: "/home".into(),
            dry: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<PushAllRequest>(&json).unwrap(),
            original
        );
    }

    #[test]
    fn pull_all_request_roundtrips() {
        let original = PullAllRequest {
            repos_file: "/repos.toml".into(),
            home_dir: "/home".into(),
            dry: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<PullAllRequest>(&json).unwrap(),
            original
        );
    }

    #[test]
    fn repo_sync_result_dto_uses_pascal_case_keys() {
        let dto = RepoSyncResultDto {
            name: "repo".into(),
            branch: "main".into(),
            status: "pushed".into(),
            detail: "ok".into(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["Name"], "repo");
        assert_eq!(json["Branch"], "main");
        assert_eq!(json["Status"], "pushed");
        assert_eq!(json["Detail"], "ok");
    }

    #[test]
    fn sync_data_roundtrips_with_every_exit_variant() {
        for exit in [SyncExitDto::Clean, SyncExitDto::Warn, SyncExitDto::Fail] {
            let original = SyncData {
                results: vec![RepoSyncResultDto {
                    name: "repo".into(),
                    branch: "main".into(),
                    status: "up-to-date".into(),
                    detail: "up to date".into(),
                }],
                exit,
            };
            let json = serde_json::to_string(&original).unwrap();
            assert_eq!(serde_json::from_str::<SyncData>(&json).unwrap(), original);
        }
    }
}
