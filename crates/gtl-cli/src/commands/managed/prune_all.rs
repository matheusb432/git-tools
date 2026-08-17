//! Fanning `prune` out across every managed repo.

use std::fmt::Write as _;

use gtl_application::projects::prune_branches::{self, PruneAction, PruneExit};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitEffectMode},
    paths::ProjectName,
};
use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRun};

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
    match super::project_catalog::load_projects() {
        Ok(repos) => {
            let execution = prune_branches::execute(
                prune_branches::PruneBranches {
                    repos,
                    onto: onto.clone(),
                    mode: if options.dry {
                        GitEffectMode::DryRun
                    } else {
                        GitEffectMode::Apply
                    },
                },
                &HybridGitClient,
            );
            project_prune_execution(onto, options.dry, options.output.is_json(), execution)
        }
        Err(error) => ManagedRun {
            exit: ManagedExit::Fail,
            results: Vec::new(),
            stdout: String::new(),
            stderr: format!("{error:#}"),
        },
    }
}

fn project_prune_execution(
    onto: &BranchName,
    dry: bool,
    json: bool,
    execution: Result<prune_branches::PruneBranchesOk, prune_branches::PruneBranchesError>,
) -> ManagedRun<PruneRepoResult> {
    match execution {
        Ok(execution) => {
            let exit = match execution.exit {
                PruneExit::Clean => ManagedExit::Clean,
                PruneExit::Warn => ManagedExit::Warn,
            };
            let results = project_repo_results(execution.results);
            let stdout = format_prune(onto, dry, json, &results);
            ManagedRun {
                exit,
                results,
                stdout,
                stderr: String::new(),
            }
        }
        Err(error) => {
            let stderr = format!("{error:#}");
            let results = match error {
                prune_branches::PruneBranchesError::Transport {
                    mut completed_results,
                    failed_result,
                    ..
                } => {
                    if let Some(failed_result) = failed_result {
                        completed_results.push(*failed_result);
                    }
                    project_repo_results(completed_results)
                }
                _ => Vec::new(),
            };
            let stdout = format_prune(onto, dry, json, &results);
            ManagedRun {
                exit: ManagedExit::Warn,
                results,
                stdout,
                stderr,
            }
        }
    }
}

fn project_repo_results(results: Vec<prune_branches::PruneRepoResult>) -> Vec<PruneRepoResult> {
    results
        .into_iter()
        .map(|result| project_repo_result(result.name, result.action))
        .collect()
}

fn project_repo_result(name: ProjectName, action: PruneAction) -> PruneRepoResult {
    match action {
        PruneAction::Absent => PruneRepoResult {
            name,
            present: false,
            deleted: Vec::new(),
            failed: Vec::new(),
            detail: "not present on this machine".into(),
        },
        PruneAction::Refused(detail) | PruneAction::Nothing(detail) => PruneRepoResult {
            name,
            present: true,
            deleted: Vec::new(),
            failed: Vec::new(),
            detail,
        },
        PruneAction::WouldDelete(branches) => PruneRepoResult {
            name,
            present: true,
            detail: format!("would delete {} branch(es)", branches.len()),
            deleted: branches
                .into_iter()
                .map(|branch| PrunedBranch {
                    name: branch.name,
                    id: branch.id,
                })
                .collect(),
            failed: Vec::new(),
        },
        PruneAction::Applied(result) => PruneRepoResult {
            name,
            present: true,
            deleted: result
                .deleted
                .iter()
                .map(|branch| PrunedBranch {
                    name: branch.name.clone(),
                    id: branch.id.clone(),
                })
                .collect(),
            failed: result
                .failed
                .iter()
                .map(|failure| failure.name.clone())
                .collect(),
            detail: crate::commands::prune::render_result(&result),
        },
    }
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
    use gtl_application::{
        projects::prune_branches::{self, PruneAction, PruneBranchesOk, PruneExit},
        repositories::{
            apply_prune::{ApplyPruneOk, PruneFailure, PruneStatus},
            plan_prune::PruneBranch,
        },
    };

    use super::*;
    use crate::testing::{branch_name, commit_id, project_name};

    #[test]
    fn application_results_project_to_the_existing_json_shape() {
        let execution = PruneBranchesOk {
            exit: PruneExit::Warn,
            results: vec![prune_branches::PruneRepoResult {
                name: project_name("api"),
                action: PruneAction::Applied(ApplyPruneOk {
                    status: PruneStatus::Partial,
                    deleted: vec![PruneBranch {
                        name: branch_name("feature/done"),
                        id: commit_id("a"),
                    }],
                    failed: vec![PruneFailure {
                        name: branch_name("feature/blocked"),
                        reason: "branch is checked out".into(),
                    }],
                }),
            }],
        };

        let run = project_prune_execution(&branch_name("main"), false, true, Ok(execution));

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
        let completed_result = prune_branches::PruneRepoResult {
            name: project_name("api"),
            action: PruneAction::Applied(ApplyPruneOk {
                status: PruneStatus::Ok,
                deleted: vec![PruneBranch {
                    name: branch_name("feature/api"),
                    id: commit_id("a"),
                }],
                failed: Vec::new(),
            }),
        };
        let failed_result = prune_branches::PruneRepoResult {
            name: project_name("web"),
            action: PruneAction::Applied(ApplyPruneOk {
                status: PruneStatus::Ok,
                deleted: vec![PruneBranch {
                    name: branch_name("feature/web"),
                    id: commit_id("b"),
                }],
                failed: Vec::new(),
            }),
        };
        let execution = Err(prune_branches::PruneBranchesError::Transport {
            failed_repo: project_name("web"),
            completed_results: vec![completed_result],
            failed_result: Some(Box::new(failed_result)),
            source: anyhow::anyhow!("git transport unavailable"),
        });

        let run = project_prune_execution(&branch_name("main"), false, false, execution);

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
