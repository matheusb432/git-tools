//! Fanning `push`/`pull` out across every managed repo via the daemon.

use std::fmt::Write as _;

use gtl_contracts::{
    envelope::{Envelope, NoteLevel, Outcome},
    managed::{
        PullAllRequest, PushAllRequest, RepoSyncResultDto, RepoSyncStatusDto, SyncData, SyncExitDto,
    },
};
use serde::Serialize;

use super::{
    ManagedExit, ManagedOptions, ManagedRun,
    manifest::resolve_manifest_location,
    push_summary::{PushOutcome, PushSummary},
};
use crate::client::HttpClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncOperation {
    Push,
    Pull,
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
        repo: String,
        status: RepoSyncStatusDto,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PushPullResult {
    pub name: String,
    pub branch: String,
    pub status: RepoSyncStatusDto,
    pub detail: String,
}

impl From<RepoSyncResultDto> for PushPullResult {
    fn from(dto: RepoSyncResultDto) -> Self {
        Self {
            name: dto.name,
            branch: dto.branch,
            status: dto.status,
            detail: dto.detail,
        }
    }
}

fn exit_from_dto(exit: SyncExitDto) -> ManagedExit {
    match exit {
        SyncExitDto::Clean => ManagedExit::Clean,
        SyncExitDto::Warn => ManagedExit::Warn,
        SyncExitDto::Fail => ManagedExit::Fail,
    }
}

pub fn run_push_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    let client = match HttpClient::ensure_daemon() {
        Ok(client) => client,
        Err(error) => return manifest_error(&error),
    };
    let (repos_file, home_dir) = match resolve_manifest_location(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(&error),
    };
    let request = PushAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match client.push_all(&request) {
        Ok(envelope) => finish(SyncOperation::Push, options, envelope),
        Err(error) => manifest_error(&error),
    }
}

pub fn run_pull_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    let client = match HttpClient::ensure_daemon() {
        Ok(client) => client,
        Err(error) => return manifest_error(&error),
    };
    let (repos_file, home_dir) = match resolve_manifest_location(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(&error),
    };
    let request = PullAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match client.pull_all(&request) {
        Ok(envelope) => finish(SyncOperation::Pull, options, envelope),
        Err(error) => manifest_error(&error),
    }
}

fn manifest_error<T>(error: &anyhow::Error) -> ManagedRun<T> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: format!("{error:#}"),
    }
}

fn finish(
    operation: SyncOperation,
    options: &ManagedOptions,
    envelope: Envelope<SyncData>,
) -> ManagedRun<PushPullResult> {
    if envelope.outcome == Outcome::Error {
        let text = envelope
            .notes
            .iter()
            .rev()
            .find(|n| n.level == NoteLevel::Error)
            .map_or_else(
                || "daemon reported an error".to_string(),
                |n| n.text.clone(),
            );
        return manifest_error(&anyhow::anyhow!(text));
    }
    let Some(data) = envelope.data else {
        return manifest_error(&anyhow::anyhow!("daemon returned ok without data"));
    };
    let results: Vec<PushPullResult> = data.results.into_iter().map(PushPullResult::from).collect();
    let exit = exit_from_dto(data.exit);
    let stdout = match format_push_pull(operation, options.dry, options.json, &results, exit) {
        Ok(stdout) => stdout,
        Err(error) => return manifest_error(&error.into()),
    };
    ManagedRun {
        exit,
        results,
        stdout,
        stderr: String::new(),
    }
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
        let _ = writeln!(
            out,
            "{:<30} {:<18} {:<12} {}",
            result.name, result.branch, result.status, result.detail
        );
    }
    if let Some(outcomes) = push_outcomes {
        let summary = PushSummary::from_outcomes(outcomes, dry);
        let _ = write!(out, "\n{}", summary.render(exit.code()));
    } else {
        let fail = results
            .iter()
            .filter(|result| result.status == RepoSyncStatusDto::Fail)
            .count();
        let warn = results
            .iter()
            .filter(|result| result.status == RepoSyncStatusDto::Warn)
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
        RepoSyncStatusDto::Pushed | RepoSyncStatusDto::WouldPush => Ok(PushOutcome::Pushed),
        RepoSyncStatusDto::Skip | RepoSyncStatusDto::UpToDate => Ok(PushOutcome::Skipped),
        RepoSyncStatusDto::Fail => Ok(PushOutcome::Failed),
        RepoSyncStatusDto::Warn => Ok(PushOutcome::Warned),
        RepoSyncStatusDto::Pulled | RepoSyncStatusDto::WouldPull => {
            Err(PushPullFormatError::PullOnlyStatus {
                repo: result.name.clone(),
                status: result.status,
            })
        }
    }
}
