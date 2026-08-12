//! Wire DTOs for the managed feature's daemon endpoints (`push-all`/`pull-all`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushAllRequest {
    pub dry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullAllRequest {
    pub dry: bool,
}

/// The stable per-repository status used by managed push and pull responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepoSyncStatusDto {
    Skip,
    UpToDate,
    Pushed,
    WouldPush,
    Pulled,
    WouldPull,
    Warn,
    Fail,
}

impl std::fmt::Display for RepoSyncStatusDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match self {
            Self::Skip => "skip",
            Self::UpToDate => "up-to-date",
            Self::Pushed => "pushed",
            Self::WouldPush => "would-push",
            Self::Pulled => "pulled",
            Self::WouldPull => "would-pull",
            Self::Warn => "warn",
            Self::Fail => "fail",
        };
        formatter.pad(token)
    }
}

/// The exact JSON shape the CLI's `--json` mode has always produced
/// (`PushPullResult`'s retired `#[serde(rename_all = "PascalCase")]`), preserved
/// so `--json` output stays byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RepoSyncResultDto {
    pub name: String,
    pub branch: String,
    pub status: RepoSyncStatusDto,
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
    fn repo_sync_result_dto_uses_pascal_case_keys() {
        let dto = RepoSyncResultDto {
            name: "repo".into(),
            branch: "main".into(),
            status: RepoSyncStatusDto::Pushed,
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
                    status: RepoSyncStatusDto::UpToDate,
                    detail: "up to date".into(),
                }],
                exit,
            };
            let json = serde_json::to_string(&original).unwrap();
            assert_eq!(serde_json::from_str::<SyncData>(&json).unwrap(), original);
        }
    }

    #[test]
    fn repo_sync_status_dto_tokens_are_byte_stable() {
        let cases = [
            (RepoSyncStatusDto::Skip, "skip"),
            (RepoSyncStatusDto::UpToDate, "up-to-date"),
            (RepoSyncStatusDto::Pushed, "pushed"),
            (RepoSyncStatusDto::WouldPush, "would-push"),
            (RepoSyncStatusDto::Pulled, "pulled"),
            (RepoSyncStatusDto::WouldPull, "would-pull"),
            (RepoSyncStatusDto::Warn, "warn"),
            (RepoSyncStatusDto::Fail, "fail"),
        ];

        for (status, token) in cases {
            assert_eq!(
                serde_json::to_string(&status).unwrap(),
                format!(r#""{token}""#)
            );
            assert_eq!(
                serde_json::from_str::<RepoSyncStatusDto>(&format!(r#""{token}""#)).unwrap(),
                status
            );
            assert_eq!(status.to_string(), token);
        }
    }

    #[test]
    fn repo_sync_status_dto_honors_display_alignment() {
        assert_eq!(format!("{:<12}", RepoSyncStatusDto::Pushed), "pushed      ");
    }

    #[test]
    fn repo_sync_status_dto_rejects_unknown_tokens() {
        let error = serde_json::from_str::<RepoSyncStatusDto>(r#""unknown""#)
            .expect_err("unknown status must fail decoding");
        assert!(error.to_string().contains("unknown variant"), "{error}");
    }
}
