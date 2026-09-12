//! Fanning `commit --all` out across every managed repo.

use anyhow::Context as _;
use gtl_models::paths::{ProjectName, RepositoryRelativePath};
pub use gtl_models::repository::working_tree::CommitFile;
use gtl_wire::v1;
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedOutput, ManagedRun};
use crate::server_client::ServerClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitAction {
    Absent,
    Clean,
    WouldCommit,
    Skipped,
    Committed,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    name: ProjectName,
    present: bool,
    dirty: bool,
    files: Vec<CommitFile>,
    action: CommitAction,
    detail: String,
}

impl CommitResult {
    fn name(&self) -> &ProjectName {
        &self.name
    }

    const fn is_present(&self) -> bool {
        self.present
    }

    const fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn files(&self) -> &[CommitFile] {
        &self.files
    }

    const fn action(&self) -> CommitAction {
        self.action
    }

    pub(super) fn annotate_push(&self, result: &mut super::PushPullResult) {
        if self.name != result.name {
            return;
        }
        let prefix = match self.action {
            CommitAction::Committed => "Committed locally",
            CommitAction::WouldCommit => {
                if result.status == super::RepoSyncStatus::UpToDate {
                    result.status = super::RepoSyncStatus::WouldPush;
                    result.detail.clear();
                }
                "Would commit all changes"
            }
            CommitAction::Absent
            | CommitAction::Clean
            | CommitAction::Skipped
            | CommitAction::Fail => return,
        };
        result.detail = if result.detail.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}; {}", result.detail)
        };
    }

    fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CommitResultJson<'a> {
    name: &'a str,
    present: bool,
    dirty: bool,
    files: &'a [CommitFile],
    action: &'static str,
    detail: &'a str,
}

fn action_wire(action: CommitAction) -> &'static str {
    match action {
        CommitAction::Absent => "absent",
        CommitAction::Clean => "clean",
        CommitAction::WouldCommit => "would-commit",
        CommitAction::Skipped => "skipped",
        CommitAction::Committed => "committed",
        CommitAction::Fail => "fail",
    }
}

#[must_use]
pub fn run_commit_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    run_commit_all_with_scope(options, false)
}

#[must_use]
pub fn run_commit_for_push_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    run_commit_all_with_scope(options, true)
}

fn run_commit_all_with_scope(
    options: &ManagedOptions,
    use_push_all_exclusions: bool,
) -> ManagedRun<CommitResult> {
    if let Some(message) = &options.message_for_all
        && message.trim().is_empty()
    {
        return usage_failure("commit --all requires a non-empty message");
    }

    if !options.dry && options.message_for_all.is_none() && !options.interactive {
        return usage_failure("commit --all requires a message unless --dry is used");
    }

    let execution = ServerClient::connect().and_then(|client| {
        client.commit_project_repositories(commit_request(options, use_push_all_exclusions))
    });
    project_commit_execution(options.output, execution)
}

fn commit_request(
    options: &ManagedOptions,
    use_push_all_exclusions: bool,
) -> v1::CommitProjectRepositoriesRequest {
    v1::CommitProjectRepositoriesRequest {
        dry_run: options.dry,
        message: options.message_for_all.clone(),
        use_push_all_exclusions,
    }
}

fn usage_failure(message: &str) -> ManagedRun<CommitResult> {
    ManagedRun {
        exit: ManagedExit::Usage,
        results: Vec::new(),
        stdout: String::new(),
        stderr: message.to_owned(),
    }
}

fn project_commit_execution(
    output: ManagedOutput,
    execution: anyhow::Result<v1::CommitProjectRepositoriesResponse>,
) -> ManagedRun<CommitResult> {
    let response = match execution {
        Ok(response) => response,
        Err(error) => return transport_failure(crate::error_text(&error)),
    };
    let results = match response
        .results
        .into_iter()
        .map(commit_result_from_grpc)
        .collect::<anyhow::Result<Vec<_>>>()
    {
        Ok(results) => results,
        Err(error) => return transport_failure(crate::error_text(&error)),
    };
    let exit = match v1::ProjectCommitExit::try_from(response.exit) {
        Ok(v1::ProjectCommitExit::Clean) => ManagedExit::Clean,
        Ok(v1::ProjectCommitExit::Warning) => ManagedExit::Warn,
        Ok(v1::ProjectCommitExit::Failed) => ManagedExit::Fail,
        Ok(v1::ProjectCommitExit::Unspecified) | Err(_) => {
            return transport_failure("gtl-server returned an invalid project commit exit".into());
        }
    };
    let stdout = format_commit(output, &results);
    ManagedRun {
        exit,
        results,
        stdout,
        stderr: response.failure_detail.unwrap_or_default(),
    }
}

fn transport_failure(message: String) -> ManagedRun<CommitResult> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: message,
    }
}

fn commit_result_from_grpc(result: v1::ProjectCommitResult) -> anyhow::Result<CommitResult> {
    Ok(CommitResult {
        name: ProjectName::try_new(result.project_name)
            .context("gtl-server returned an empty project name")?,
        present: result.present,
        dirty: result.dirty,
        files: result
            .files
            .into_iter()
            .map(|file| {
                Ok(CommitFile {
                    status: file.status,
                    path: RepositoryRelativePath::try_new(file.path.into())
                        .context("gtl-server returned an invalid changed-file path")?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
        action: match v1::ProjectCommitAction::try_from(result.action) {
            Ok(v1::ProjectCommitAction::Absent) => CommitAction::Absent,
            Ok(v1::ProjectCommitAction::Clean) => CommitAction::Clean,
            Ok(v1::ProjectCommitAction::WouldCommit) => CommitAction::WouldCommit,
            Ok(v1::ProjectCommitAction::Skipped) => CommitAction::Skipped,
            Ok(v1::ProjectCommitAction::Committed) => CommitAction::Committed,
            Ok(v1::ProjectCommitAction::Failed) => CommitAction::Fail,
            Ok(v1::ProjectCommitAction::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid project commit action")
            }
        },
        detail: result.detail,
    })
}

fn format_commit(output: ManagedOutput, results: &[CommitResult]) -> String {
    if output.is_json() {
        let projected = results
            .iter()
            .map(|result| CommitResultJson {
                name: result.name().as_str(),
                present: result.is_present(),
                dirty: result.is_dirty(),
                files: result.files(),
                action: action_wire(result.action()),
                detail: result.detail(),
            })
            .collect::<Vec<_>>();
        return serde_json::to_string_pretty(&projected).unwrap_or_else(|_| "[]".to_string());
    }

    let rows = results
        .iter()
        .map(|result| {
            let detail = match result.action {
                CommitAction::Absent | CommitAction::Clean | CommitAction::Committed => {
                    String::new()
                }
                CommitAction::WouldCommit => format!("{} files (all changes)", result.files.len()),
                CommitAction::Skipped | CommitAction::Fail => result.detail.clone(),
            };
            [
                result.name.to_string(),
                action_label(result.action).to_string(),
                detail,
            ]
        })
        .collect::<Vec<_>>();
    let table = crate::output::table(
        ["Project", "Result", "Detail"],
        &rows,
        output.color_enabled(),
    );
    let mut counts = Vec::new();
    for action in [
        CommitAction::Committed,
        CommitAction::WouldCommit,
        CommitAction::Clean,
        CommitAction::Skipped,
        CommitAction::Absent,
        CommitAction::Fail,
    ] {
        let count = results
            .iter()
            .filter(|result| result.action == action)
            .count();
        if count > 0 {
            counts.push(format!("{count} {}", action_label(action).to_lowercase()));
        }
    }
    format!(
        "{table}\n\n{}: {}",
        crate::output::count_label(results.len(), "project", "projects"),
        if counts.is_empty() {
            "nothing to commit".into()
        } else {
            counts.join(", ")
        }
    )
}

fn action_label(action: CommitAction) -> &'static str {
    match action {
        CommitAction::Absent => "Absent",
        CommitAction::Clean => "Clean",
        CommitAction::WouldCommit => "Would commit",
        CommitAction::Skipped => "Skipped",
        CommitAction::Committed => "Committed",
        CommitAction::Fail => "Failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{project_name, repository_relative_path};

    fn committed_result() -> CommitResult {
        CommitResult {
            name: project_name("api"),
            present: true,
            dirty: true,
            files: vec![CommitFile {
                status: "M".into(),
                path: repository_relative_path("src/lib.rs"),
            }],
            action: CommitAction::Committed,
            detail: "[main abc1234] save".into(),
        }
    }

    fn push_result(
        status: super::super::RepoSyncStatus,
        detail: &str,
    ) -> super::super::PushPullResult {
        super::super::PushPullResult {
            name: project_name("api"),
            branch: None,
            status,
            detail: detail.into(),
        }
    }

    #[test]
    fn combined_preview_replaces_the_precommit_noop() {
        use super::super::RepoSyncStatus;

        let mut commit = committed_result();
        commit.action = CommitAction::WouldCommit;
        let mut push = push_result(RepoSyncStatus::UpToDate, "up to date");

        commit.annotate_push(&mut push);

        assert_eq!(push.status, RepoSyncStatus::WouldPush);
        assert_eq!(push.detail, "Would commit all changes");
    }

    #[test]
    fn combined_preview_preserves_push_failures_and_warnings() {
        use super::super::RepoSyncStatus;

        let mut commit = committed_result();
        commit.action = CommitAction::WouldCommit;
        for status in [RepoSyncStatus::Fail, RepoSyncStatus::Warn] {
            let mut push = push_result(status, "remote unavailable");

            commit.annotate_push(&mut push);

            assert_eq!(push.status, status);
            assert_eq!(push.detail, "Would commit all changes; remote unavailable");
        }
    }

    #[test]
    fn combined_failure_keeps_the_completed_local_commit_visible() {
        use super::super::RepoSyncStatus;

        let mut push = push_result(RepoSyncStatus::Fail, "remote unavailable");
        committed_result().annotate_push(&mut push);

        assert_eq!(push.status, RepoSyncStatus::Fail);
        assert_eq!(push.detail, "Committed locally; remote unavailable");
    }

    #[test]
    fn commit_action_wire_tokens_are_byte_stable() {
        assert_eq!(action_wire(CommitAction::Absent), "absent");
        assert_eq!(action_wire(CommitAction::Clean), "clean");
        assert_eq!(action_wire(CommitAction::WouldCommit), "would-commit");
        assert_eq!(action_wire(CommitAction::Skipped), "skipped");
        assert_eq!(action_wire(CommitAction::Committed), "committed");
        assert_eq!(action_wire(CommitAction::Fail), "fail");
    }

    #[test]
    fn push_all_commit_request_enables_push_exclusions_only_for_that_scope() {
        let options = ManagedOptions {
            dry: true,
            output: super::super::ManagedOutput::Json,
            message_for_all: Some("save".into()),
            interactive: false,
        };

        assert!(!commit_request(&options, false).use_push_all_exclusions);
        assert!(commit_request(&options, true).use_push_all_exclusions);
    }

    #[test]
    fn commit_json_projection_is_byte_stable_and_omits_server_metadata() {
        let results = vec![committed_result()];

        assert_eq!(
            format_commit(ManagedOutput::Json, &results),
            concat!(
                "[\n",
                "  {\n",
                "    \"Name\": \"api\",\n",
                "    \"Present\": true,\n",
                "    \"Dirty\": true,\n",
                "    \"Files\": [\n",
                "      {\n",
                "        \"Status\": \"M\",\n",
                "        \"Path\": \"src/lib.rs\"\n",
                "      }\n",
                "    ],\n",
                "    \"Action\": \"committed\",\n",
                "    \"Detail\": \"[main abc1234] save\"\n",
                "  }\n",
                "]",
            )
        );
    }

    #[test]
    fn transport_failure_projection_preserves_completed_stdout_and_stderr() {
        let response = v1::CommitProjectRepositoriesResponse {
            results: vec![v1::ProjectCommitResult {
                project_name: "api".into(),
                present: true,
                dirty: true,
                files: vec![v1::CommitFile {
                    status: "M".into(),
                    path: "src/lib.rs".into(),
                }],
                action: v1::ProjectCommitAction::Committed as i32,
                detail: "[main abc1234] save".into(),
            }],
            exit: v1::ProjectCommitExit::Failed as i32,
            failure_detail: Some(
                "project commit failed for 'web': git transport unavailable".into(),
            ),
        };

        let run = project_commit_execution(ManagedOutput::Text { color: false }, Ok(response));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.results, vec![committed_result()]);
        assert_eq!(
            run.stdout,
            concat!(
                "Project  Result     Detail\n",
                "api      Committed\n",
                "\n1 project: 1 committed",
            )
        );
        assert_eq!(
            run.stderr,
            "project commit failed for 'web': git transport unavailable"
        );
    }
}
