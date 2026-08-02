//! Fanning `push`/`pull` out across every managed repo via the daemon.

use std::{fmt::Write as _, path::PathBuf};

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
use crate::client::{Backend, HttpBackend};

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
    match HttpBackend::ensure_daemon() {
        Ok(backend) => run_push_all_with(&backend, options),
        Err(error) => manifest_error(&error),
    }
}

pub(crate) fn run_push_all_with(
    backend: &impl Backend,
    options: &ManagedOptions,
) -> ManagedRun<PushPullResult> {
    run_push_all_resolving_with(backend, options, resolve_manifest_location)
}

fn run_push_all_resolving_with(
    backend: &impl Backend,
    options: &ManagedOptions,
    resolve_manifest: impl FnOnce(&ManagedOptions) -> anyhow::Result<(PathBuf, PathBuf)>,
) -> ManagedRun<PushPullResult> {
    let (repos_file, home_dir) = match resolve_manifest(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(&error),
    };
    let req = PushAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match backend.push_all(&req) {
        Ok(envelope) => finish(SyncOperation::Push, options, envelope),
        Err(error) => manifest_error(&error),
    }
}

pub fn run_pull_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    match HttpBackend::ensure_daemon() {
        Ok(backend) => run_pull_all_with(&backend, options),
        Err(error) => manifest_error(&error),
    }
}

pub(crate) fn run_pull_all_with(
    backend: &impl Backend,
    options: &ManagedOptions,
) -> ManagedRun<PushPullResult> {
    let (repos_file, home_dir) = match resolve_manifest_location(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(&error),
    };
    let req = PullAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match backend.pull_all(&req) {
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
    format_push_pull_with(operation, dry, json, results, exit, |results| {
        serde_json::to_string_pretty(results)
    })
}

fn format_push_pull_with(
    operation: SyncOperation,
    dry: bool,
    json: bool,
    results: &[PushPullResult],
    exit: ManagedExit,
    serialize: impl FnOnce(&[PushPullResult]) -> serde_json::Result<String>,
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
        return serialize(results).map_err(PushPullFormatError::SerializeJson);
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

#[cfg(test)]
mod tests {
    use gtl_contracts::{
        envelope::{Envelope, Note, NoteLevel, Outcome},
        managed::{
            PullAllRequest, PushAllRequest, RepoSyncResultDto, RepoSyncStatusDto, SyncData,
            SyncExitDto,
        },
    };
    use tempfile::TempDir;

    use super::*;
    use crate::client::Backend;

    #[derive(Default)]
    struct FakeBackend {
        push_response: Option<Envelope<SyncData>>,
        pull_response: Option<Envelope<SyncData>>,
    }

    impl Backend for FakeBackend {
        fn push_all(&self, _req: &PushAllRequest) -> anyhow::Result<Envelope<SyncData>> {
            Ok(self.push_response.clone().expect("push_response scripted"))
        }
        fn pull_all(&self, _req: &PullAllRequest) -> anyhow::Result<Envelope<SyncData>> {
            Ok(self.pull_response.clone().expect("pull_response scripted"))
        }
    }

    fn ok_envelope(
        statuses: &[(&str, RepoSyncStatusDto)],
        exit: SyncExitDto,
    ) -> Envelope<SyncData> {
        Envelope {
            outcome: Outcome::Ok,
            notes: Vec::new(),
            data: Some(SyncData {
                results: statuses
                    .iter()
                    .map(|(name, status)| RepoSyncResultDto {
                        name: (*name).into(),
                        branch: "main".into(),
                        status: *status,
                        detail: "detail".into(),
                    })
                    .collect(),
                exit,
            }),
        }
    }

    fn options(directory: &TempDir, dry: bool, json: bool) -> ManagedOptions {
        ManagedOptions {
            repos_file: Some(directory.path().join("repos.toml")),
            home_dir: Some(directory.path().join("home")),
            dry,
            json,
            color: false,
            message_for_all: None,
            interactive: false,
        }
    }

    #[test]
    fn run_push_all_with_maps_the_envelope_into_a_managed_run() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(ok_envelope(
                &[("repo", RepoSyncStatusDto::Pushed)],
                SyncExitDto::Clean,
            )),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, false, false));

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].status, RepoSyncStatusDto::Pushed);
        assert!(run.stdout.contains("push -> repo"));
        assert!(run.stdout.contains("REPO"));
    }

    #[test]
    fn run_pull_all_with_json_mode_prints_the_pascal_case_array() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            pull_response: Some(ok_envelope(
                &[("repo", RepoSyncStatusDto::Pulled)],
                SyncExitDto::Warn,
            )),
            ..Default::default()
        };

        let run = run_pull_all_with(&backend, &options(&directory, false, true));

        assert_eq!(run.exit, ManagedExit::Warn);
        assert!(run.stdout.contains("\"Name\": \"repo\""));
        assert!(run.stdout.contains("\"Status\": \"pulled\""));
    }

    #[test]
    fn managed_push_summary_reports_pushed_and_skipped_without_zero_noise() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(ok_envelope(
                &[
                    ("pushed", RepoSyncStatusDto::Pushed),
                    ("current", RepoSyncStatusDto::UpToDate),
                    ("absent", RepoSyncStatusDto::Skip),
                ],
                SyncExitDto::Clean,
            )),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, false, false));

        assert!(
            run.stdout
                .ends_with("exit 0  -  3 repos: 1 pushed, 2 skipped")
        );
        assert!(!run.stdout.contains("0 fail"));
        assert!(!run.stdout.contains("0 warn"));
    }

    #[test]
    fn managed_push_summary_reports_dry_and_nonzero_problem_counts() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(ok_envelope(
                &[
                    ("candidate", RepoSyncStatusDto::WouldPush),
                    ("current", RepoSyncStatusDto::UpToDate),
                    ("broken", RepoSyncStatusDto::Fail),
                    ("detached", RepoSyncStatusDto::Warn),
                ],
                SyncExitDto::Fail,
            )),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, true, false));

        assert!(
            run.stdout
                .ends_with("exit 2  -  4 repos: 1 would push, 1 skipped, 1 fail, 1 warn")
        );
    }

    #[test]
    fn pull_only_status_in_push_response_is_a_contextual_error() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(ok_envelope(
                &[("wrong-repo", RepoSyncStatusDto::Pulled)],
                SyncExitDto::Clean,
            )),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, false, false));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.stderr.contains("wrong-repo"), "{}", run.stderr);
        assert!(run.stderr.contains("pulled"), "{}", run.stderr);
    }

    #[test]
    fn pull_only_status_in_json_push_response_is_a_contextual_error() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(ok_envelope(
                &[("wrong-json-repo", RepoSyncStatusDto::WouldPull)],
                SyncExitDto::Clean,
            )),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, false, true));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.stderr.contains("wrong-json-repo"), "{}", run.stderr);
        assert!(run.stderr.contains("would-pull"), "{}", run.stderr);
    }

    #[test]
    fn json_serialization_failure_is_a_typed_format_error() {
        let serialization_error = serde_json::from_str::<serde_json::Value>("{")
            .expect_err("fixture must be invalid JSON");

        let error = format_push_pull_with(
            SyncOperation::Push,
            false,
            true,
            &[],
            ManagedExit::Clean,
            |_| Err(serialization_error),
        )
        .expect_err("serialization failure must propagate");

        assert!(matches!(&error, PushPullFormatError::SerializeJson(_)));
        assert!(
            error
                .to_string()
                .contains("failed to serialize managed sync results")
        );
    }

    #[test]
    fn managed_pull_human_summary_remains_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            pull_response: Some(ok_envelope(
                &[("repo", RepoSyncStatusDto::Warn)],
                SyncExitDto::Warn,
            )),
            ..Default::default()
        };

        let run = run_pull_all_with(&backend, &options(&directory, false, false));

        let expected_row = format!("{:<30} {:<18} {:<12} {}", "repo", "main", "warn", "detail");
        let actual_row = run
            .stdout
            .lines()
            .find(|line| line.starts_with("repo"))
            .expect("managed pull output should contain the repository table row");
        assert_eq!(actual_row, expected_row);
        assert!(run.stdout.ends_with("exit 1  -  1 repos: 0 fail, 1 warn"));
    }

    #[test]
    fn a_daemon_error_outcome_becomes_stderr_and_a_fail_exit() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeBackend {
            push_response: Some(Envelope {
                outcome: Outcome::Error,
                notes: vec![Note {
                    level: NoteLevel::Error,
                    text: "manifest exploded".into(),
                }],
                data: None,
            }),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&directory, false, false));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.stderr, "manifest exploded");
    }

    #[test]
    fn a_manifest_resolution_error_never_reaches_the_backend() {
        let directory = tempfile::tempdir().unwrap();
        let options = options(&directory, false, false);
        let backend = FakeBackend::default();

        let run = run_push_all_resolving_with(&backend, &options, |_| {
            Err(anyhow::anyhow!("managed-repos manifest not found"))
        });

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.stderr.contains("managed-repos manifest not found"));
    }
}
