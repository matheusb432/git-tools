//! Fanning `prune` out across every managed repo.

use std::fmt::Write as _;

use anyhow::Context as _;
use gtl_models::{diffs::CommitId, git::BranchName, paths::ProjectName};
use gtl_wire::v1;
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRun};
use crate::{
    commands::prune::{PruneFailure, PruneResult, PruneStatus},
    server_client::ServerClient,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PrunedBranch {
    pub name: BranchName,
    pub id: CommitId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PruneRepoResult {
    pub name: ProjectName,
    pub present: bool,
    /// Branches deleted (or, in dry mode, that would be deleted).
    pub deleted: Vec<PrunedBranch>,
    /// Names of branches that failed to delete.
    pub failed: Vec<BranchName>,
    pub detail: String,
}

/// Fan out `prune` over every managed repo. With `options.dry` (set when `--all` is used
/// without `-y`) each repo only reports what *would* be deleted; otherwise branches are
/// deleted. Any non-clean repo (a delete failure or a refusal) maps to [`ManagedExit::Warn`].
pub fn run_prune_all(onto: &BranchName, options: &ManagedOptions) -> ManagedRun<PruneRepoResult> {
    let execution = ServerClient::connect().and_then(|client| {
        client.prune_project_branches(v1::PruneProjectBranchesRequest {
            onto_branch: onto.to_string(),
            dry_run: options.dry,
        })
    });
    project_prune_execution(onto, options.dry, options.output.is_json(), execution)
}

fn project_prune_execution(
    onto: &BranchName,
    dry: bool,
    json: bool,
    execution: anyhow::Result<v1::PruneProjectBranchesResponse>,
) -> ManagedRun<PruneRepoResult> {
    let response = match execution {
        Ok(response) => response,
        Err(error) => return prune_failure(format!("{error:#}")),
    };
    let results = match response
        .results
        .into_iter()
        .map(project_repo_result)
        .collect::<anyhow::Result<Vec<_>>>()
    {
        Ok(results) => results,
        Err(error) => return prune_failure(format!("{error:#}")),
    };
    let exit = match v1::ProjectPruneExit::try_from(response.exit) {
        Ok(v1::ProjectPruneExit::Clean) => ManagedExit::Clean,
        Ok(v1::ProjectPruneExit::Warning) => ManagedExit::Warn,
        Ok(v1::ProjectPruneExit::Unspecified) | Err(_) => {
            return prune_failure("gtl-server returned an invalid project prune exit".into());
        }
    };
    let stdout = format_prune(onto, dry, json, &results);
    ManagedRun {
        exit,
        results,
        stdout,
        stderr: response.failure_detail.unwrap_or_default(),
    }
}

fn prune_failure(message: String) -> ManagedRun<PruneRepoResult> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: message,
    }
}

fn project_repo_result(result: v1::ProjectPruneResult) -> anyhow::Result<PruneRepoResult> {
    let name = ProjectName::try_new(result.project_name)
        .context("gtl-server returned an empty project name")?;
    let action = v1::ProjectPruneAction::try_from(result.action)
        .context("gtl-server returned an invalid project prune action")?;
    let deleted = result
        .deleted
        .into_iter()
        .map(pruned_branch)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let failures = result
        .failures
        .into_iter()
        .map(prune_failure_from_grpc)
        .collect::<anyhow::Result<Vec<_>>>()?;

    match action {
        v1::ProjectPruneAction::Absent => Ok(PruneRepoResult {
            name,
            present: false,
            deleted: Vec::new(),
            failed: Vec::new(),
            detail: "not present on this machine".into(),
        }),
        v1::ProjectPruneAction::Refused | v1::ProjectPruneAction::Nothing => Ok(PruneRepoResult {
            name,
            present: true,
            deleted: Vec::new(),
            failed: Vec::new(),
            detail: result.detail,
        }),
        v1::ProjectPruneAction::WouldDelete => Ok(PruneRepoResult {
            name,
            present: true,
            detail: format!("would delete {} branch(es)", deleted.len()),
            deleted,
            failed: Vec::new(),
        }),
        v1::ProjectPruneAction::Applied => {
            let application_result = PruneResult {
                status: if failures.is_empty() {
                    PruneStatus::Ok
                } else if deleted.is_empty() {
                    PruneStatus::Failed
                } else {
                    PruneStatus::Partial
                },
                deleted: deleted
                    .iter()
                    .map(|branch| crate::commands::prune::PruneBranch {
                        name: branch.name.clone(),
                        id: branch.id.clone(),
                    })
                    .collect(),
                failed: failures.clone(),
                failure_detail: None,
            };
            Ok(PruneRepoResult {
                name,
                present: true,
                deleted,
                failed: failures.into_iter().map(|failure| failure.name).collect(),
                detail: crate::commands::prune::render_result(&application_result),
            })
        }
        v1::ProjectPruneAction::Unspecified => {
            anyhow::bail!("gtl-server returned an unspecified project prune action")
        }
    }
}

fn pruned_branch(branch: v1::PruneBranch) -> anyhow::Result<PrunedBranch> {
    Ok(PrunedBranch {
        name: BranchName::try_new(branch.name)
            .context("gtl-server returned an empty pruned branch name")?,
        id: CommitId::try_from(branch.commit_id)
            .context("gtl-server returned an invalid pruned branch commit ID")?,
    })
}

fn prune_failure_from_grpc(failure: v1::PruneFailure) -> anyhow::Result<PruneFailure> {
    Ok(PruneFailure {
        name: BranchName::try_new(failure.name)
            .context("gtl-server returned an empty failed branch name")?,
        reason: failure.reason,
    })
}

fn format_prune(onto: &BranchName, dry: bool, json: bool, results: &[PruneRepoResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let verb = if dry { "dry prune" } else { "prune" };
    let mut out = format!("{verb} (merged into '{onto}')\n\n");
    let _ = writeln!(out, "{:<30} {:<10} DETAIL", "REPO", "BRANCHES");
    for result in results {
        let count = if result.failed.is_empty() {
            result.deleted.len().to_string()
        } else {
            format!("{}!{}", result.deleted.len(), result.failed.len())
        };
        let _ = writeln!(
            out,
            "{:<30} {:<10} {}",
            result.name,
            count,
            result.detail.lines().next().unwrap_or("")
        );
    }
    let deleted: usize = results.iter().map(|result| result.deleted.len()).sum();
    let failed: usize = results.iter().map(|result| result.failed.len()).sum();
    let _ = write!(
        out,
        "\n{} repos: {} {}, {} failed",
        results.len(),
        deleted,
        if dry { "to delete" } else { "deleted" },
        failed
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{branch_name, commit_id};

    #[test]
    fn grpc_results_project_to_the_existing_json_shape() {
        let response = v1::PruneProjectBranchesResponse {
            results: vec![v1::ProjectPruneResult {
                project_name: "api".into(),
                action: v1::ProjectPruneAction::Applied as i32,
                deleted: vec![v1::PruneBranch {
                    name: "feature/done".into(),
                    commit_id: commit_id("a").to_string(),
                }],
                failures: vec![v1::PruneFailure {
                    name: "feature/blocked".into(),
                    reason: "branch is checked out".into(),
                }],
                detail: String::new(),
            }],
            exit: v1::ProjectPruneExit::Warning as i32,
            failure_detail: None,
        };

        let run = project_prune_execution(&branch_name("main"), false, true, Ok(response));

        assert_eq!(run.exit, ManagedExit::Warn);
        assert_eq!(
            run.stdout,
            concat!(
                "[\n",
                "  {\n",
                "    \"Name\": \"api\",\n",
                "    \"Present\": true,\n",
                "    \"Deleted\": [\n",
                "      {\n",
                "        \"Name\": \"feature/done\",\n",
                "        \"Id\": \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n",
                "      }\n",
                "    ],\n",
                "    \"Failed\": [\n",
                "      \"feature/blocked\"\n",
                "    ],\n",
                "    \"Detail\": \"deleted 1 branch.\\nrecover: git branch feature/done aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\nfailed: feature/blocked — branch is checked out\"\n",
                "  }\n",
                "]",
            )
        );
    }

    #[test]
    fn transport_failure_projection_preserves_completed_stdout_and_stderr() {
        let response = v1::PruneProjectBranchesResponse {
            results: vec!["api", "web"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| v1::ProjectPruneResult {
                    project_name: name.into(),
                    action: v1::ProjectPruneAction::Applied as i32,
                    deleted: vec![v1::PruneBranch {
                        name: format!("feature/{name}"),
                        commit_id: if index == 0 {
                            commit_id("a").to_string()
                        } else {
                            commit_id("b").to_string()
                        },
                    }],
                    failures: Vec::new(),
                    detail: String::new(),
                })
                .collect(),
            exit: v1::ProjectPruneExit::Warning as i32,
            failure_detail: Some(
                "project prune failed for 'web': git transport unavailable".into(),
            ),
        };

        let run = project_prune_execution(&branch_name("main"), false, false, Ok(response));

        assert_eq!(run.exit, ManagedExit::Warn);
        assert_eq!(run.results.len(), 2);
        assert_eq!(
            run.stdout,
            concat!(
                "prune (merged into 'main')\n",
                "\n",
                "REPO                           BRANCHES   DETAIL\n",
                "api                            1          deleted 1 branch.\n",
                "web                            1          deleted 1 branch.\n",
                "\n2 repos: 2 deleted, 0 failed",
            )
        );
        assert_eq!(
            run.stderr,
            "project prune failed for 'web': git transport unavailable"
        );
    }
}
