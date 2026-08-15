//! Presentation for `prune` confirmation and structured application results.

use std::fmt::Write as _;

use gtl_application::branches::{apply_prune::ApplyPruneOk, plan_prune::PruneBranch};
use gtl_models::diffs::CommitIdAbbreviation;

/// Renders the destructive branch-prune confirmation block.
pub fn confirmation(onto: &str, branches: &[PruneBranch]) -> String {
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
pub fn render_result(result: &ApplyPruneOk) -> String {
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
    use gtl_application::branches::{
        apply_prune::{ApplyPruneOk, PruneFailure, PruneStatus},
        plan_prune::PruneBranch,
    };

    use super::*;
    use crate::testing::commit_id;

    #[test]
    fn confirmation_renders_the_selected_branches() {
        let branches = vec![PruneBranch {
            name: "feature/done".into(),
            id: commit_id("a"),
        }];

        assert_eq!(
            confirmation("main", &branches),
            "will delete 1 branch(es) merged into 'main':\n  feature/done  aaaaaaa"
        );
    }

    #[test]
    fn result_rendering_includes_recovery_and_failure_lines() {
        let result = ApplyPruneOk {
            status: PruneStatus::Partial,
            deleted: vec![
                PruneBranch {
                    name: "feature/first".into(),
                    id: commit_id("a"),
                },
                PruneBranch {
                    name: "feature/second".into(),
                    id: commit_id("b"),
                },
            ],
            failed: vec![PruneFailure {
                name: "fix/blocked".into(),
                reason: "branch is checked out".into(),
            }],
        };

        assert_eq!(
            render_result(&result),
            "deleted 2 branches.\nrecover: git branch feature/first aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nrecover: git branch feature/second bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nfailed: fix/blocked — branch is checked out"
        );
    }
}
