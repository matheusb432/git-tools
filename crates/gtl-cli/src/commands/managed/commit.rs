//! Fanning `commit --all` out across every managed repo.

use std::fmt::Write as _;

pub use gtl_application::managed::commit_all::CommitResult;
use gtl_application::managed::commit_all::{self, CommitAction, CommitExit};
use gtl_infra::git_client::HybridGitClient;
pub use gtl_models::managed::working_tree::CommitFile;
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRun};

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

const fn managed_exit(exit: CommitExit) -> ManagedExit {
    match exit {
        CommitExit::Clean => ManagedExit::Clean,
        CommitExit::Warn => ManagedExit::Warn,
        CommitExit::Fail => ManagedExit::Fail,
    }
}

pub fn run_commit_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    if let Some(message) = &options.message_for_all
        && message.trim().is_empty()
    {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit --all requires a non-empty message".to_string(),
        };
    }

    if !options.dry && options.message_for_all.is_none() && !options.interactive {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit --all requires a message unless --dry is used".to_string(),
        };
    }

    match super::project_catalog::load_projects() {
        Ok(repos) => {
            let execution = commit_all::execute(
                commit_all::CommitAll {
                    repos,
                    message: options.message_for_all.clone(),
                    dry: options.dry,
                },
                &HybridGitClient,
            );
            project_commit_execution(options.output.is_json(), execution)
        }
        Err(error) => ManagedRun {
            exit: ManagedExit::Fail,
            results: Vec::new(),
            stdout: String::new(),
            stderr: format!("{error:#}"),
        },
    }
}

fn project_commit_execution(
    json: bool,
    execution: Result<commit_all::CommitAllOk, commit_all::CommitAllError>,
) -> ManagedRun<CommitResult> {
    match execution {
        Ok(result) => {
            let exit = managed_exit(result.exit);
            let stdout = format_commit(exit, json, &result.results);
            ManagedRun {
                exit,
                results: result.results,
                stdout,
                stderr: String::new(),
            }
        }
        Err(error) => {
            let stderr = format!("{error:#}");
            let results = match error {
                commit_all::CommitAllError::Transport {
                    mut completed_results,
                    failed_result,
                    ..
                } => {
                    if let Some(failed_result) = failed_result {
                        completed_results.push(*failed_result);
                    }
                    completed_results
                }
                _ => Vec::new(),
            };
            let exit = ManagedExit::Fail;
            let stdout = format_commit(exit, json, &results);
            ManagedRun {
                exit,
                results,
                stdout,
                stderr,
            }
        }
    }
}

fn format_commit(exit: ManagedExit, json: bool, results: &[CommitResult]) -> String {
    if json {
        let projected = results
            .iter()
            .map(|result| CommitResultJson {
                name: &result.name,
                present: result.present,
                dirty: result.dirty,
                files: &result.files,
                action: action_wire(result.action),
                detail: &result.detail,
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
            result.name,
            action_wire(result.action),
            result.detail
        );
    }
    let _ = write!(out, "\nexit {}  -  {} repos", exit.code(), results.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::commit_id;

    #[test]
    fn commit_action_wire_tokens_are_byte_stable() {
        // The `--json` `Action` field and the action-table column emit these tokens;
        // changing one is a breaking output change, so pin every variant.
        assert_eq!(action_wire(CommitAction::Absent), "absent");
        assert_eq!(action_wire(CommitAction::Clean), "clean");
        assert_eq!(action_wire(CommitAction::WouldCommit), "would-commit");
        assert_eq!(action_wire(CommitAction::Skipped), "skipped");
        assert_eq!(action_wire(CommitAction::Committed), "committed");
        assert_eq!(action_wire(CommitAction::Fail), "fail");
    }

    #[test]
    fn commit_json_projection_is_byte_stable_and_omits_application_metadata() {
        let results = vec![CommitResult {
            name: "api".into(),
            present: true,
            dirty: true,
            files: vec![CommitFile {
                status: "M".into(),
                path: "src/lib.rs".into(),
            }],
            action: CommitAction::Committed,
            detail: "[main abc1234] save".into(),
            id: Some(commit_id("a")),
            staged: true,
        }];

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
        let completed_result = CommitResult {
            name: "api".into(),
            present: true,
            dirty: true,
            files: vec![CommitFile {
                status: "M".into(),
                path: "src/lib.rs".into(),
            }],
            action: CommitAction::Committed,
            detail: "[main abc1234] save".into(),
            id: Some(commit_id("a")),
            staged: true,
        };
        let execution = Err(commit_all::CommitAllError::Transport {
            failed_repo: "web".into(),
            completed_results: vec![completed_result.clone()],
            failed_result: None,
            source: anyhow::anyhow!("git transport unavailable"),
        });

        let run = project_commit_execution(false, execution);

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.results, vec![completed_result]);
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
            "managed commit failed for 'web': git transport unavailable"
        );
    }
}
