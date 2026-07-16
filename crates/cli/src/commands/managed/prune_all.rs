//! Fanning `prune` out across every managed repo.

use std::{fmt::Write as _, path::Path};

use serde::Serialize;

use super::{ManagedExit, ManagedOptions, ManagedRepo, ManagedRun};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PrunedBranch {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PruneRepoResult {
    pub name: String,
    pub present: bool,
    /// Branches deleted (or, in dry mode, that would be deleted).
    pub deleted: Vec<PrunedBranch>,
    /// Names of branches that failed to delete.
    pub failed: Vec<String>,
    pub detail: String,
}

/// Fan out `prune` over every managed repo. With `options.dry` (set when `--all` is used
/// without `-y`) each repo only reports what *would* be deleted; otherwise branches are
/// deleted. Any non-clean repo (a delete failure or a refusal) maps to [`ManagedExit::Warn`].
pub fn run_prune_all(onto: &str, options: &ManagedOptions) -> ManagedRun<PruneRepoResult> {
    match super::manifest::load_repos(options) {
        Ok(repos) => {
            let runner = infra::git_runner::StdGitRunner;
            let results = repos
                .iter()
                .map(|repo| prune_one(&runner, repo, onto, options.dry))
                .collect::<Vec<_>>();
            let exit = if results.iter().any(|result| !result.failed.is_empty()) {
                ManagedExit::Warn
            } else {
                ManagedExit::Clean
            };
            let stdout = format_prune(onto, options.dry, options.json, &results);
            ManagedRun {
                exit,
                results,
                stdout,
                stderr: String::new(),
            }
        }
        Err(error) => ManagedRun {
            exit: ManagedExit::Fail,
            results: Vec::new(),
            stdout: String::new(),
            stderr: format!("{error:#}"),
        },
    }
}

fn prune_one(
    runner: &impl application::ports::GitRunner,
    repo: &ManagedRepo,
    onto: &str,
    dry: bool,
) -> PruneRepoResult {
    use crate::commands::prune::{self, PrunePlan};

    let mut result = PruneRepoResult {
        name: repo.name.clone(),
        present: false,
        deleted: Vec::new(),
        failed: Vec::new(),
        detail: String::new(),
    };
    if !repo.path.join(".git").exists() {
        result.detail = "not present on this machine".to_string();
        return result;
    }
    result.present = true;

    match prune::plan(runner, &repo.path, onto) {
        PrunePlan::Refused(detail) | PrunePlan::Nothing(detail) => result.detail = detail,
        PrunePlan::Ready { top, branches, .. } => {
            if dry {
                result.deleted = branches
                    .iter()
                    .map(|branch| PrunedBranch {
                        name: branch.name.clone(),
                        sha: branch.sha.clone(),
                    })
                    .collect();
                result.detail = format!("would delete {} branch(es)", branches.len());
            } else {
                let applied = prune::apply(runner, Path::new(&top), &branches);
                result.deleted = applied
                    .deleted
                    .iter()
                    .map(|branch| PrunedBranch {
                        name: branch.name.clone(),
                        sha: branch.sha.clone(),
                    })
                    .collect();
                result.failed = applied
                    .failed
                    .iter()
                    .map(|branch| branch.name.clone())
                    .collect();
                result.detail = applied.detail;
            }
        }
    }
    result
}

fn format_prune(onto: &str, dry: bool, json: bool, results: &[PruneRepoResult]) -> String {
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
