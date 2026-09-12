//! Fanning `push`/`pull` out across every managed repo via `gtl-server`.

use std::fmt::Write as _;

use gtl_models::{git::BranchName, paths::ProjectName};
use gtl_wire::v1;
use serde::Serialize;

use super::{
    ManagedExit, ManagedOptions, ManagedOutput, ManagedRun,
    push_summary::{PushOutcome, PushSummary},
};
use crate::server_client::ServerClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncOperation {
    Push,
    Pull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepoSyncStatus {
    Skip,
    UpToDate,
    Pushed,
    WouldPush,
    Pulled,
    WouldPull,
    Warn,
    Fail,
}

impl std::fmt::Display for RepoSyncStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match self {
            Self::Skip => "Skipped",
            Self::UpToDate => "Up to date",
            Self::Pushed => "Pushed",
            Self::WouldPush => "Would push",
            Self::Pulled => "Pulled",
            Self::WouldPull => "Would pull",
            Self::Warn => "Warning",
            Self::Fail => "Failed",
        };
        formatter.pad(token)
    }
}

#[derive(Debug, thiserror::Error)]
enum PushPullFormatError {
    #[error("failed to serialize managed sync results")]
    SerializeJson(#[source] serde_json::Error),
    #[error(
        "repository `{repo}` returned pull-only status `{status}` while formatting push results"
    )]
    PullOnlyStatus {
        repo: ProjectName,
        status: RepoSyncStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PushPullResult {
    pub name: ProjectName,
    pub branch: Option<BranchName>,
    pub status: RepoSyncStatus,
    pub detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PushAllReport<'a> {
    selected: &'a [PushPullResult],
    excluded: &'a [ProjectName],
}

fn exit_from_grpc(exit: v1::ProjectSyncExit) -> anyhow::Result<ManagedExit> {
    match exit {
        v1::ProjectSyncExit::Clean => Ok(ManagedExit::Clean),
        v1::ProjectSyncExit::Warning => Ok(ManagedExit::Warn),
        v1::ProjectSyncExit::Failed => Ok(ManagedExit::Fail),
        v1::ProjectSyncExit::Unspecified => {
            anyhow::bail!("gtl-server returned an unspecified project sync exit")
        }
    }
}

#[must_use]
pub fn run_push_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => return managed_error(&error),
    };
    let request = v1::PushProjectRepositoriesRequest {
        dry_run: options.dry,
    };
    match client.push_project_repositories(request) {
        Ok(response) => finish(SyncOperation::Push, options, response),
        Err(error) => managed_error(&error),
    }
}

#[must_use]
pub fn run_pull_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => return managed_error(&error),
    };
    let request = v1::PullProjectRepositoriesRequest {
        dry_run: options.dry,
    };
    match client.pull_project_repositories(request) {
        Ok(response) => finish(SyncOperation::Pull, options, response),
        Err(error) => managed_error(&error),
    }
}

fn managed_error<T>(error: &anyhow::Error) -> ManagedRun<T> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: crate::error_text(error),
    }
}

fn finish(
    operation: SyncOperation,
    options: &ManagedOptions,
    response: impl Into<ProjectRepositorySyncSummary>,
) -> ManagedRun<PushPullResult> {
    let response = response.into();
    let results = match response
        .results
        .into_iter()
        .map(result_from_grpc)
        .collect::<anyhow::Result<Vec<_>>>()
    {
        Ok(results) => results,
        Err(error) => return managed_error(&error),
    };
    let Ok(excluded) = response
        .excluded_project_names
        .into_iter()
        .map(ProjectName::try_new)
        .collect::<Result<Vec<_>, _>>()
    else {
        return managed_error(&anyhow::anyhow!(
            "gtl-server returned an empty excluded project name"
        ));
    };
    let exit = match v1::ProjectSyncExit::try_from(response.exit)
        .map_err(|_| anyhow::anyhow!("gtl-server returned an unknown project sync exit"))
        .and_then(exit_from_grpc)
    {
        Ok(exit) => exit,
        Err(error) => return managed_error(&error),
    };
    let stdout = match format_push_pull(operation, options.dry, options.output, &results, &excluded)
    {
        Ok(stdout) => stdout,
        Err(error) => return managed_error(&error.into()),
    };
    ManagedRun {
        exit,
        results,
        stdout,
        stderr: String::new(),
    }
}

struct ProjectRepositorySyncSummary {
    results: Vec<v1::RepositorySyncResult>,
    excluded_project_names: Vec<String>,
    exit: i32,
}

impl From<v1::PushProjectRepositoriesResponse> for ProjectRepositorySyncSummary {
    fn from(response: v1::PushProjectRepositoriesResponse) -> Self {
        Self {
            results: response.selected,
            excluded_project_names: response.excluded_project_names,
            exit: response.exit,
        }
    }
}

impl From<v1::PullProjectRepositoriesResponse> for ProjectRepositorySyncSummary {
    fn from(response: v1::PullProjectRepositoriesResponse) -> Self {
        Self {
            results: response.results,
            excluded_project_names: Vec::new(),
            exit: response.exit,
        }
    }
}

impl TryFrom<v1::RepositorySyncResult> for PushPullResult {
    type Error = anyhow::Error;

    fn try_from(result: v1::RepositorySyncResult) -> anyhow::Result<Self> {
        result_from_grpc(result)
    }
}

fn result_from_grpc(result: v1::RepositorySyncResult) -> anyhow::Result<PushPullResult> {
    let status = match v1::RepositorySyncStatus::try_from(result.status)
        .map_err(|_| anyhow::anyhow!("gtl-server returned an unknown repository sync status"))?
    {
        v1::RepositorySyncStatus::Unspecified => {
            anyhow::bail!("gtl-server returned an unspecified repository sync status")
        }
        v1::RepositorySyncStatus::Skip => RepoSyncStatus::Skip,
        v1::RepositorySyncStatus::UpToDate => RepoSyncStatus::UpToDate,
        v1::RepositorySyncStatus::Pushed => RepoSyncStatus::Pushed,
        v1::RepositorySyncStatus::WouldPush => RepoSyncStatus::WouldPush,
        v1::RepositorySyncStatus::Pulled => RepoSyncStatus::Pulled,
        v1::RepositorySyncStatus::WouldPull => RepoSyncStatus::WouldPull,
        v1::RepositorySyncStatus::Warning => RepoSyncStatus::Warn,
        v1::RepositorySyncStatus::Failed => RepoSyncStatus::Fail,
    };
    Ok(PushPullResult {
        name: ProjectName::try_new(result.project_name)
            .map_err(|_| anyhow::anyhow!("gtl-server returned an empty project name"))?,
        branch: result
            .branch
            .map(BranchName::try_new)
            .transpose()
            .map_err(|_| anyhow::anyhow!("gtl-server returned an empty branch name"))?,
        status,
        detail: result.detail,
    })
}

fn format_push_pull(
    operation: SyncOperation,
    dry: bool,
    output: ManagedOutput,
    results: &[PushPullResult],
    excluded: &[ProjectName],
) -> Result<String, PushPullFormatError> {
    let push_outcomes = match operation {
        SyncOperation::Push => Some(
            results
                .iter()
                .map(push_outcome)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        SyncOperation::Pull => None,
    };

    if output.is_json() {
        return match operation {
            SyncOperation::Push => serde_json::to_string_pretty(&PushAllReport {
                selected: results,
                excluded,
            }),
            SyncOperation::Pull => serde_json::to_string_pretty(results),
        }
        .map_err(PushPullFormatError::SerializeJson);
    }

    let rows = results
        .iter()
        .map(|result| {
            let detail = if matches!(
                result.detail.as_str(),
                "up to date" | "up to date (already synced)"
            ) {
                String::new()
            } else {
                result.detail.clone()
            };
            [
                result.name.to_string(),
                result
                    .branch
                    .as_ref()
                    .map_or_else(|| "-".into(), ToString::to_string),
                result.status.to_string(),
                detail,
            ]
        })
        .chain(excluded.iter().map(|project| {
            [
                project.to_string(),
                "-".into(),
                "Excluded".into(),
                String::new(),
            ]
        }))
        .collect::<Vec<_>>();
    let mut out = crate::output::table(
        ["Project", "Branch", "Result", "Detail"],
        &rows,
        output.color_enabled(),
    );
    let summary = push_outcomes.map_or_else(
        || pull_summary(results, dry),
        |outcomes| PushSummary::from_outcomes(outcomes, dry, excluded.len()).render(),
    );
    let _ = write!(out, "\n\n{summary}");
    Ok(out)
}

fn pull_summary(results: &[PushPullResult], dry: bool) -> String {
    let count = |status| {
        results
            .iter()
            .filter(|result| result.status == status)
            .count()
    };
    let pulled = count(RepoSyncStatus::Pulled) + count(RepoSyncStatus::WouldPull);
    let verb = if dry { "would pull" } else { "pulled" };
    let warnings = count(RepoSyncStatus::Warn);
    let mut summary = format!(
        "{}: {pulled} {verb}",
        crate::output::count_label(results.len(), "project", "projects")
    );
    for (label, count) in [
        ("up to date", count(RepoSyncStatus::UpToDate)),
        ("skipped", count(RepoSyncStatus::Skip)),
        ("failed", count(RepoSyncStatus::Fail)),
        (if warnings == 1 { "warning" } else { "warnings" }, warnings),
    ] {
        if count > 0 {
            let _ = write!(summary, ", {count} {label}");
        }
    }
    summary
}

fn push_outcome(result: &PushPullResult) -> Result<PushOutcome, PushPullFormatError> {
    match result.status {
        RepoSyncStatus::Pushed | RepoSyncStatus::WouldPush => Ok(PushOutcome::Pushed),
        RepoSyncStatus::Skip => Ok(PushOutcome::Skipped),
        RepoSyncStatus::UpToDate => Ok(PushOutcome::UpToDate),
        RepoSyncStatus::Fail => Ok(PushOutcome::Failed),
        RepoSyncStatus::Warn => Ok(PushOutcome::Warned),
        RepoSyncStatus::Pulled | RepoSyncStatus::WouldPull => {
            Err(PushPullFormatError::PullOnlyStatus {
                repo: result.name.clone(),
                status: result.status,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn project(name: &str) -> ProjectName {
        ProjectName::try_new(name).unwrap()
    }

    fn selected(status: RepoSyncStatus) -> PushPullResult {
        PushPullResult {
            name: project("example-project"),
            branch: Some(BranchName::try_new("main").unwrap()),
            status,
            detail: "ahead by 1".into(),
        }
    }

    #[test]
    fn dry_push_text_reports_selected_and_excluded_repositories() {
        let output = format_push_pull(
            SyncOperation::Push,
            true,
            ManagedOutput::Text { color: false },
            &[selected(RepoSyncStatus::WouldPush)],
            &[project("excluded-project")],
        )
        .unwrap();

        assert_eq!(output.matches("example-project").count(), 1);
        assert_eq!(output.matches("excluded-project").count(), 1);
        assert!(output.contains("Would push"));
        assert!(output.contains("2 projects: 1 would push, 1 excluded"));
    }

    #[test]
    fn push_json_separates_selected_results_from_excluded_names() {
        let output = format_push_pull(
            SyncOperation::Push,
            false,
            ManagedOutput::Json,
            &[selected(RepoSyncStatus::Pushed)],
            &[project("excluded-project")],
        )
        .unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output).unwrap(),
            json!({
                "Selected": [{
                    "Name": "example-project",
                    "Branch": "main",
                    "Status": "pushed",
                    "Detail": "ahead by 1"
                }],
                "Excluded": ["excluded-project"]
            })
        );
    }

    #[test]
    fn pull_json_keeps_the_existing_result_array() {
        let output = format_push_pull(
            SyncOperation::Pull,
            false,
            ManagedOutput::Json,
            &[selected(RepoSyncStatus::Pulled)],
            &[],
        )
        .unwrap();

        assert!(
            serde_json::from_str::<serde_json::Value>(&output)
                .unwrap()
                .is_array()
        );
    }
}
