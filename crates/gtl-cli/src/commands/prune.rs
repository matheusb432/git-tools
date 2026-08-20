//! Presentation for `prune` confirmation and structured application results.

use std::fmt::Write as _;

use anyhow::Context as _;
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    git::BranchName,
};
use gtl_wire::v1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneBranch {
    pub name: BranchName,
    pub id: CommitId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneStatus {
    Ok,
    Partial,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneFailure {
    pub name: BranchName,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneResult {
    pub status: PruneStatus,
    pub deleted: Vec<PruneBranch>,
    pub failed: Vec<PruneFailure>,
    pub failure_detail: Option<String>,
}

pub(crate) fn plan_from_grpc(plan: v1::RepositoryPrunePlan) -> anyhow::Result<Vec<PruneBranch>> {
    plan.branches.into_iter().map(branch_from_grpc).collect()
}

pub(crate) fn plan_to_grpc(
    repository_root: String,
    branches: &[PruneBranch],
) -> v1::RepositoryPrunePlan {
    v1::RepositoryPrunePlan {
        repository_root,
        branches: branches.iter().map(branch_to_grpc).collect(),
    }
}

pub(crate) fn result_from_grpc(
    response: v1::ExecuteRepositoryPruneResponse,
) -> anyhow::Result<PruneResult> {
    let status = match v1::RepositoryPruneStatus::try_from(response.status) {
        Ok(v1::RepositoryPruneStatus::Ok) => PruneStatus::Ok,
        Ok(v1::RepositoryPruneStatus::Partial) => PruneStatus::Partial,
        Ok(v1::RepositoryPruneStatus::Failed) => PruneStatus::Failed,
        Ok(v1::RepositoryPruneStatus::Aborted) => PruneStatus::Aborted,
        Ok(v1::RepositoryPruneStatus::Unspecified) | Err(_) => {
            anyhow::bail!("gtl-server returned an invalid prune status")
        }
    };
    Ok(PruneResult {
        status,
        deleted: response
            .deleted
            .into_iter()
            .map(branch_from_grpc)
            .collect::<anyhow::Result<Vec<_>>>()?,
        failed: response
            .failures
            .into_iter()
            .map(|failure| {
                Ok(PruneFailure {
                    name: BranchName::try_new(failure.name)
                        .context("gtl-server returned an empty failed branch name")?,
                    reason: failure.reason,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
        failure_detail: response.failure_detail,
    })
}

fn branch_from_grpc(branch: v1::PruneBranch) -> anyhow::Result<PruneBranch> {
    Ok(PruneBranch {
        name: BranchName::try_new(branch.name)
            .context("gtl-server returned an empty prune branch name")?,
        id: CommitId::try_from(branch.commit_id)
            .context("gtl-server returned an invalid prune commit ID")?,
    })
}

fn branch_to_grpc(branch: &PruneBranch) -> v1::PruneBranch {
    v1::PruneBranch {
        name: branch.name.to_string(),
        commit_id: branch.id.to_string(),
    }
}

/// Renders the destructive branch-prune confirmation block.
pub fn confirmation(onto: &BranchName, branches: &[PruneBranch]) -> String {
    let mut text = format!(
        "will delete {} branch(es) merged into '{onto}':",
        branches.len()
    );
    for branch in branches {
        let _ = write!(
            text,
            "\n  {}  {}",
            branch.name,
            branch.id.abbreviated(CommitIdAbbreviation::SevenCharacters)
        );
    }
    text
}

/// Renders deleted-branch recovery commands and per-branch failures.
pub fn render_result(result: &PruneResult) -> String {
    let mut detail = format!(
        "deleted {} branch{}.",
        result.deleted.len(),
        plural(result.deleted.len())
    );
    for branch in &result.deleted {
        let _ = write!(
            detail,
            "\nrecover: git branch {} {}",
            branch.name, branch.id
        );
    }
    for failure in &result.failed {
        let _ = write!(detail, "\nfailed: {} — {}", failure.name, failure.reason);
    }
    detail
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "es" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{branch_name, commit_id};

    #[test]
    fn confirmation_renders_the_selected_branches() {
        let branches = vec![PruneBranch {
            name: branch_name("feature/done"),
            id: commit_id("a"),
        }];

        assert_eq!(
            confirmation(&branch_name("main"), &branches),
            "will delete 1 branch(es) merged into 'main':\n  feature/done  aaaaaaa"
        );
    }

    #[test]
    fn result_rendering_includes_recovery_and_failure_lines() {
        let result = PruneResult {
            status: PruneStatus::Partial,
            deleted: vec![
                PruneBranch {
                    name: branch_name("feature/first"),
                    id: commit_id("a"),
                },
                PruneBranch {
                    name: branch_name("feature/second"),
                    id: commit_id("b"),
                },
            ],
            failed: vec![PruneFailure {
                name: branch_name("fix/blocked"),
                reason: "branch is checked out".into(),
            }],
            failure_detail: None,
        };

        assert_eq!(
            render_result(&result),
            "deleted 2 branches.\nrecover: git branch feature/first aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nrecover: git branch feature/second bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nfailed: fix/blocked — branch is checked out"
        );
    }
}
