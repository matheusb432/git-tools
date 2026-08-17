//! Fanning `commit --all` out across every managed repo.

use std::fmt::Write as _;

pub use gtl_application::projects::commit_repositories::CommitResult;
use gtl_application::projects::commit_repositories::{
    self, CommitAction, CommitExit, CommitRepositoriesMode,
};
use gtl_infra::git_client::HybridGitClient;
pub use gtl_models::repository::working_tree::CommitFile;
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
    detail: String,
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
            let execution = commit_repositories::execute(
                commit_repositories::CommitRepositories {
                    repos,
                    mode: if options.dry {
                        CommitRepositoriesMode::DryRun
                    } else {
                        CommitRepositoriesMode::Apply {
                            message: options.message_for_all.clone(),
                        }
                    },
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
    execution: Result<
        commit_repositories::CommitRepositoriesOk,
        commit_repositories::CommitRepositoriesError,
    >,
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
                commit_repositories::CommitRepositoriesError::Transport {
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
                name: result.name(),
                present: result.is_present(),
                dirty: result.is_dirty(),
                files: result.files(),
                action: action_wire(result.action()),
                detail: result.detail().into_owned(),
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
    use gtl_application::projects::commit_repositories::CommitOutcome;
    use gtl_models::repository::working_tree::ChangedFiles;

    use super::*;
    use crate::testing::{commit_id, project_name, repository_relative_path};

    fn committed_result() -> CommitResult {
        CommitResult::new(
            project_name("api"),
            CommitOutcome::Committed {
                files: ChangedFiles::try_new(vec![CommitFile {
                    status: "M".into(),
                    path: repository_relative_path("src/lib.rs"),
                }])
                .expect("fixture changed files are non-empty"),
                detail: "[main abc1234] save".into(),
                id: commit_id("a"),
            },
        )
    }

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
        let completed_result = committed_result();
        let execution = Err(commit_repositories::CommitRepositoriesError::Transport {
            failed_repo: project_name("web"),
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
            "project commit failed for 'web': git transport unavailable"
        );
    }
}
