//! Fanning `commit --all` out across every managed repo.

use std::fmt::Write as _;

use anyhow::Context as _;
use gtl_models::paths::{ProjectName, RepositoryRelativePath};
pub use gtl_models::repository::working_tree::CommitFile;
use gtl_wire::v1;
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRun};
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

pub fn run_commit_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    run_commit_all_with_scope(options, false)
}

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
    project_commit_execution(options.output.is_json(), execution)
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
    json: bool,
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
    let stdout = format_commit(exit, json, &results);
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

fn format_commit(exit: ManagedExit, json: bool, results: &[CommitResult]) -> String {
    if json {
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

    let mut out = String::new();
    let _ = writeln!(out, "{:<30} {:<14} DETAIL", "REPO", "ACTION");
    for result in results {
        let _ = writeln!(
            out,
            "{:<30} {:<14} {}",
            result.name(),
            action_wire(result.action()),
            result.detail()
        );
    }
    let _ = write!(out, "\nexit {}  -  {} repos", exit.code(), results.len());
    out
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
            format_commit(ManagedExit::Clean, true, &results),
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

        let run = project_commit_execution(false, Ok(response));

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.results, vec![committed_result()]);
        assert_eq!(
            run.stdout,
            concat!(
                "REPO                           ACTION         DETAIL\n",
                "api                            committed      [main abc1234] save\n",
                "\nexit 2  -  1 repos",
            )
        );
        assert_eq!(
            run.stderr,
            "project commit failed for 'web': git transport unavailable"
        );
    }
}
