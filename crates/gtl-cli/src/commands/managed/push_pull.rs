//! Fanning `push`/`pull` out across every managed repo via the daemon.

use std::fmt::Write as _;

use gtl_models::{git::BranchName, paths::ProjectName};
use gtl_wire::v1;
use serde::Serialize;

use super::{
    ManagedExit, ManagedOptions, ManagedRun,
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

impl SyncOperation {
    const fn label(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Pull => "pull",
        }
    }

    const fn arrow(self) -> &'static str {
        match self {
            Self::Push => "->",
            Self::Pull => "<-",
        }
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
    let exit = match v1::ProjectSyncExit::try_from(response.exit)
        .map_err(|_| anyhow::anyhow!("gtl-server returned an unknown project sync exit"))
        .and_then(exit_from_grpc)
    {
        Ok(exit) => exit,
        Err(error) => return managed_error(&error),
    };
    let stdout = match format_push_pull(
        operation,
        options.dry,
        options.output.is_json(),
        &results,
        exit,
    ) {
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
    exit: i32,
}

impl From<v1::PushProjectRepositoriesResponse> for ProjectRepositorySyncSummary {
    fn from(response: v1::PushProjectRepositoriesResponse) -> Self {
        Self {
            results: response.results,
            exit: response.exit,
        }
    }
}

impl From<v1::PullProjectRepositoriesResponse> for ProjectRepositorySyncSummary {
    fn from(response: v1::PullProjectRepositoriesResponse) -> Self {
        Self {
            results: response.results,
            exit: response.exit,
        }
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
    json: bool,
    results: &[PushPullResult],
    exit: ManagedExit,
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

    if json {
        return serde_json::to_string_pretty(results).map_err(PushPullFormatError::SerializeJson);
    }

    let mut out = String::new();
    for result in results {
        let verb = if dry {
            format!("dry {}", operation.label())
        } else {
            operation.label().to_string()
        };
        let _ = writeln!(out, "{verb} {} {}", operation.arrow(), result.name);
    }
    out.push('\n');
    let _ = writeln!(
        out,
        "{:<30} {:<18} {:<12} DETAIL",
        "REPO", "BRANCH", "STATUS"
    );
    for result in results {
        let branch = result.branch.as_ref().map_or("-", AsRef::as_ref);
        let _ = writeln!(
            out,
            "{:<30} {:<18} {:<12} {}",
            result.name, branch, result.status, result.detail
        );
    }
    if let Some(outcomes) = push_outcomes {
        let summary = PushSummary::from_outcomes(outcomes, dry);
        let _ = write!(out, "\n{}", summary.render(exit.code()));
    } else {
        let fail = results
            .iter()
            .filter(|result| result.status == RepoSyncStatus::Fail)
            .count();
        let warn = results
            .iter()
            .filter(|result| result.status == RepoSyncStatus::Warn)
            .count();
        let _ = write!(
            out,
            "\nexit {}  -  {} repos: {} fail, {} warn",
            exit.code(),
            results.len(),
            fail,
            warn
        );
    }
    Ok(out)
}

fn push_outcome(result: &PushPullResult) -> Result<PushOutcome, PushPullFormatError> {
    match result.status {
        RepoSyncStatus::Pushed | RepoSyncStatus::WouldPush => Ok(PushOutcome::Pushed),
        RepoSyncStatus::Skip | RepoSyncStatus::UpToDate => Ok(PushOutcome::Skipped),
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
