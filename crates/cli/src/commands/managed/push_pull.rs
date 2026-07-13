//! Fanning `push`/`pull` out across every managed repo via the daemon.

use std::{fmt::Write as _, path::PathBuf};

use contracts::{
    envelope::{Envelope, NoteLevel, Outcome},
    managed::{PullAllRequest, PushAllRequest, RepoSyncResultDto, SyncData, SyncExitDto},
};
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRun, manifest::resolve_manifest_location};
use crate::client::{Backend, HttpBackend};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PushPullResult {
    pub name: String,
    pub branch: String,
    pub status: String,
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
        Ok(envelope) => finish("push", options, envelope),
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
        Ok(envelope) => finish("pull", options, envelope),
        Err(error) => manifest_error(&error),
    }
}

/// Still used by `commit.rs`'s own git-output parsing — kept here (not moved during this
/// task's daemon rewire) since that module's local git logic is untouched by this plan.
pub(super) fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
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
    label: &str,
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
    let stdout = format_push_pull(label, options.dry, options.json, &results, exit);
    ManagedRun {
        exit,
        results,
        stdout,
        stderr: String::new(),
    }
}

fn format_push_pull(
    label: &str,
    dry: bool,
    json: bool,
    results: &[PushPullResult],
    exit: ManagedExit,
) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let mut out = String::new();
    for result in results {
        let verb = if dry {
            format!("dry {label}")
        } else {
            label.to_string()
        };
        let arrow = if label == "pull" { "<-" } else { "->" };
        let _ = writeln!(out, "{verb} {arrow} {}", result.name);
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
    let fail = results.iter().filter(|r| r.status == "fail").count();
    let warn = results.iter().filter(|r| r.status == "warn").count();
    let _ = write!(
        out,
        "\nexit {}  -  {} repos: {} fail, {} warn",
        exit.code(),
        results.len(),
        fail,
        warn
    );
    out
}

#[cfg(test)]
mod tests {
    use contracts::{
        envelope::{Envelope, Note, NoteLevel, Outcome},
        managed::{PullAllRequest, PushAllRequest, RepoSyncResultDto, SyncData, SyncExitDto},
    };

    use super::*;
    use crate::{client::Backend, commands::managed::test_support::ManagedFixture};

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

    fn ok_envelope(status: &str, exit: SyncExitDto) -> Envelope<SyncData> {
        Envelope {
            outcome: Outcome::Ok,
            notes: Vec::new(),
            data: Some(SyncData {
                results: vec![RepoSyncResultDto {
                    name: "repo".into(),
                    branch: "main".into(),
                    status: status.into(),
                    detail: "detail".into(),
                }],
                exit,
            }),
        }
    }

    fn options(fixture: &ManagedFixture, dry: bool, json: bool) -> ManagedOptions {
        ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry,
            json,
            color: false,
            message_for_all: None,
            interactive: false,
        }
    }

    #[test]
    fn run_push_all_with_maps_the_envelope_into_a_managed_run() {
        let fixture = ManagedFixture::new("push-thin");
        fixture.write_manifest(&[("repo", "")]);
        let backend = FakeBackend {
            push_response: Some(ok_envelope("pushed", SyncExitDto::Clean)),
            ..Default::default()
        };

        let run = run_push_all_with(&backend, &options(&fixture, false, false));

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].status, "pushed");
        assert!(run.stdout.contains("push -> repo"));
        assert!(run.stdout.contains("REPO"));
    }

    #[test]
    fn run_pull_all_with_json_mode_prints_the_pascal_case_array() {
        let fixture = ManagedFixture::new("pull-thin-json");
        fixture.write_manifest(&[("repo", "")]);
        let backend = FakeBackend {
            pull_response: Some(ok_envelope("pulled", SyncExitDto::Warn)),
            ..Default::default()
        };

        let run = run_pull_all_with(&backend, &options(&fixture, false, true));

        assert_eq!(run.exit, ManagedExit::Warn);
        assert!(run.stdout.contains("\"Name\": \"repo\""));
        assert!(run.stdout.contains("\"Status\": \"pulled\""));
    }

    #[test]
    fn a_daemon_error_outcome_becomes_stderr_and_a_fail_exit() {
        let fixture = ManagedFixture::new("push-error-outcome");
        fixture.write_manifest(&[("repo", "")]);
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

        let run = run_push_all_with(&backend, &options(&fixture, false, false));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.stderr, "manifest exploded");
    }

    #[test]
    fn a_manifest_resolution_error_never_reaches_the_backend() {
        let fixture = ManagedFixture::new("push-no-manifest");
        let options = options(&fixture, false, false);
        let backend = FakeBackend::default();

        let run = run_push_all_resolving_with(&backend, &options, |_| {
            Err(anyhow::anyhow!("managed-repos manifest not found"))
        });

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.stderr.contains("managed-repos manifest not found"));
    }
}
