//! Fanning `push`/`pull` out across every managed repo via the daemon.

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
        Err(error) => manifest_error(error),
    }
}

pub(crate) fn run_push_all_with(
    backend: &impl Backend,
    options: &ManagedOptions,
) -> ManagedRun<PushPullResult> {
    let (repos_file, home_dir) = match resolve_manifest_location(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(error),
    };
    let req = PushAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match backend.push_all(&req) {
        Ok(envelope) => finish("push", options, envelope),
        Err(error) => manifest_error(error),
    }
}

pub fn run_pull_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    match HttpBackend::ensure_daemon() {
        Ok(backend) => run_pull_all_with(&backend, options),
        Err(error) => manifest_error(error),
    }
}

pub(crate) fn run_pull_all_with(
    backend: &impl Backend,
    options: &ManagedOptions,
) -> ManagedRun<PushPullResult> {
    let (repos_file, home_dir) = match resolve_manifest_location(options) {
        Ok(location) => location,
        Err(error) => return manifest_error(error),
    };
    let req = PullAllRequest {
        repos_file: repos_file.to_string_lossy().into_owned(),
        home_dir: home_dir.to_string_lossy().into_owned(),
        dry: options.dry,
    };
    match backend.pull_all(&req) {
        Ok(envelope) => finish("pull", options, envelope),
        Err(error) => manifest_error(error),
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

fn manifest_error<T>(error: anyhow::Error) -> ManagedRun<T> {
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
        return manifest_error(anyhow::anyhow!(text));
    }
    let Some(data) = envelope.data else {
        return manifest_error(anyhow::anyhow!("daemon returned ok without data"));
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
        out.push_str(&format!("{verb} {arrow} {}\n", result.name));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<30} {:<18} {:<12} {}\n",
        "REPO", "BRANCH", "STATUS", "DETAIL"
    ));
    for result in results {
        out.push_str(&format!(
            "{:<30} {:<18} {:<12} {}\n",
            result.name, result.branch, result.status, result.detail
        ));
    }
    let fail = results.iter().filter(|r| r.status == "fail").count();
    let warn = results.iter().filter(|r| r.status == "warn").count();
    out.push_str(&format!(
        "\nexit {}  -  {} repos: {} fail, {} warn",
        exit.code(),
        results.len(),
        fail,
        warn
    ));
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use contracts::{
        envelope::{Envelope, Note, NoteLevel, Outcome},
        managed::{PullAllRequest, PushAllRequest, RepoSyncResultDto, SyncData, SyncExitDto},
    };

    use super::*;
    use crate::{client::Backend, commands::managed::test_support::ManagedFixture};

    /// Guards tests that mutate `HOME` (process-global env var). `resolve_repos_file`'s
    /// home-default fallback reads real ambient `HOME` directly — it has no seam for
    /// `ManagedOptions::home_dir` — so a "manifest unresolvable" test must pin `HOME` to a
    /// directory with no `tools/sample_project/repos.toml`, or it silently finds whatever
    /// real manifest the *running machine* happens to have at that default location.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard: pins `HOME` for the guarded test, restoring the prior value on drop
    /// (even on panic/assertion failure) so later tests see the real environment again.
    struct HomeOverride {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
    }

    impl HomeOverride {
        fn set(value: &std::path::Path) -> Self {
            let lock = ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let previous = std::env::var_os("HOME");
            // SAFETY: guarded by ENV_LOCK; no other thread touches HOME concurrently.
            unsafe { std::env::set_var("HOME", value) };
            Self {
                _lock: lock,
                previous,
            }
        }
    }

    impl Drop for HomeOverride {
        fn drop(&mut self) {
            // SAFETY: guarded by ENV_LOCK; no other thread touches HOME concurrently.
            match &self.previous {
                Some(value) => unsafe { std::env::set_var("HOME", value) },
                None => unsafe { std::env::remove_var("HOME") },
            }
        }
    }

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
    fn an_unresolvable_manifest_never_reaches_the_backend() {
        let fixture = ManagedFixture::new("push-no-manifest");
        let nowhere = fixture.root.join("nowhere");
        let mut options = options(&fixture, false, false);
        options.repos_file = None;
        options.home_dir = Some(nowhere.clone());
        let backend = FakeBackend::default();
        // Pin real HOME to the same empty dir: `resolve_repos_file`'s home-default
        // fallback reads ambient HOME directly, not `options.home_dir`.
        let _home = HomeOverride::set(&nowhere);

        let run = run_push_all_with(&backend, &options);

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.stderr.contains("managed-repos manifest not found"));
    }
}
