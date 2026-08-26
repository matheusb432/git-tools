//! Classifies repositories from local head, upstream, and working-tree facts.

use std::collections::BTreeMap;

use gtl_models::{
    git::{GitHead, GitRange},
    paths::RepositoryRoot,
    repository::{
        PathCount,
        status::{StatusChanges, StatusHead, StatusResult, StatusUpstream},
        traversal::RepositoryTarget,
        working_tree::DirtyState,
    },
};

use crate::ports::{
    GitClient, GitEffect, GitStatusSnapshot, GitStatusUpstream, GitWorkingTree,
    GitWorkingTreeSummary,
};

/// Classifies each repository from local refs without fetching.
///
/// Transport and Git rejections become explicit unavailable facts so an unreadable repository is
/// never reported as clean.
#[cqrsy::query]
pub fn execute(repos: Vec<RepositoryTarget>, git: &impl GitClient) -> Vec<StatusResult> {
    repos.into_iter().map(|repo| get_one(repo, git)).collect()
}

pub(crate) fn execute_with_known_descendants(
    repos: Vec<RepositoryTarget>,
    git: &impl GitClient,
) -> Vec<StatusResult> {
    let mut indexed_repos = repos.into_iter().enumerate().collect::<Vec<_>>();
    indexed_repos.sort_by_key(|(_, repo)| std::cmp::Reverse(repo.path.components().count()));

    let mut known_descendants = BTreeMap::new();
    let mut results = Vec::with_capacity(indexed_repos.len());
    for (index, repo) in indexed_repos {
        let path = repo.path.clone();
        let (result, summary) = get_one_with_known_descendants(repo, git, &known_descendants);
        if let Some(summary) = summary {
            known_descendants.insert(path, summary);
        }
        results.push((index, result));
    }
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

pub(crate) fn get_one(repo: RepositoryTarget, git: &impl GitClient) -> StatusResult {
    get_one_from_snapshot(repo, git, |path| git.status_snapshot(path)).0
}

fn get_one_with_known_descendants(
    repo: RepositoryTarget,
    git: &impl GitClient,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> (StatusResult, Option<GitWorkingTreeSummary>) {
    get_one_from_snapshot(repo, git, |path| {
        git.status_snapshot_with_known_descendants(path, known_descendants)
    })
}

fn get_one_from_snapshot(
    repo: RepositoryTarget,
    git: &impl GitClient,
    read_snapshot: impl FnOnce(&RepositoryRoot) -> anyhow::Result<GitEffect<GitStatusSnapshot>>,
) -> (StatusResult, Option<GitWorkingTreeSummary>) {
    let RepositoryTarget { path, label } = repo;
    if !git.repo_present(&path) {
        return (StatusResult::absent(label), None);
    }

    match read_snapshot(&path) {
        Ok(GitEffect::Applied(snapshot)) => {
            let GitStatusSnapshot {
                head,
                upstream,
                working_tree,
            } = snapshot;
            let summary = GitWorkingTreeSummary::from_working_tree(&working_tree);
            (
                StatusResult::present(
                    label,
                    snapshot_head(head, upstream),
                    snapshot_changes(&working_tree),
                ),
                Some(summary),
            )
        }
        Ok(GitEffect::Rejected(_)) | Err(_) => (
            StatusResult::present(
                label,
                fallback_head(git, &path),
                fallback_changes(git, &path),
            ),
            None,
        ),
    }
}

fn snapshot_head(head: GitHead, upstream: Option<GitStatusUpstream>) -> StatusHead {
    match head {
        GitHead::Detached => StatusHead::Detached,
        GitHead::Branch(name) => StatusHead::Branch {
            name,
            upstream: upstream.map_or(StatusUpstream::Missing, |upstream| {
                StatusUpstream::Tracking {
                    reference: upstream.reference,
                    ahead: upstream.ahead,
                }
            }),
        },
    }
}

fn snapshot_changes(working_tree: &GitWorkingTree) -> StatusChanges {
    changes_from_files(&working_tree.files)
}

fn changes_from_files(files: &[gtl_models::repository::working_tree::CommitFile]) -> StatusChanges {
    let untracked = files.iter().filter(|file| file.is_untracked()).count();
    let tracked = files.len().saturating_sub(untracked);
    StatusChanges::from_counts(PathCount::from_len(tracked), PathCount::from_len(untracked))
}

fn fallback_head(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusHead {
    match git.current_branch(repo_path) {
        Ok(GitHead::Detached) => StatusHead::Detached,
        Ok(GitHead::Branch(name)) => StatusHead::Branch {
            name,
            upstream: fallback_upstream(git, repo_path),
        },
        Err(_) => StatusHead::Unavailable,
    }
}

fn fallback_upstream(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusUpstream {
    let Ok(GitEffect::Applied(reference)) = git.upstream(repo_path) else {
        return StatusUpstream::Missing;
    };
    StatusUpstream::Tracking {
        reference,
        ahead: git
            .commit_count(repo_path, &GitRange::upstream_to_head())
            .ok()
            .flatten()
            .unwrap_or_default(),
    }
}

fn fallback_changes(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusChanges {
    let state =
        super::working_tree::read(git, repo_path).unwrap_or_else(|error| DirtyState::Unavailable {
            detail: error.to_string(),
        });
    match state {
        DirtyState::Clean => StatusChanges::Clean,
        DirtyState::Dirty(files) => changes_from_files(files.as_slice()),
        DirtyState::Unavailable { .. } | DirtyState::Absent => StatusChanges::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::{CommitCount, GitRefName},
        repository::status::{
            RepositoryStatus, StatusChanges, StatusClass, StatusHead, StatusUpstream,
        },
    };

    use super::*;
    use crate::{
        repositories::get_repository_statuses,
        utils::{ScriptedGitClient, branch_name},
    };

    fn repo(name: &str) -> RepositoryTarget {
        RepositoryTarget {
            label: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!("/repos/{name}")),
        }
    }

    #[test]
    fn absent_repo_classifies_without_calling_git() {
        let runner = ScriptedGitClient::default();
        runner
            .absent_repos
            .lock()
            .unwrap()
            .push("/repos/api".into());

        let results = get_repository_statuses::execute(vec![repo("api")], &runner);

        assert_eq!(results[0].class(), StatusClass::Absent);
        assert_eq!(results[0].detail(), "not present");
        assert!(!results[0].is_present());
    }

    #[test]
    fn clean_synced_repo_classifies_as_clean_checkmark() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::applied(""),
        ]);

        let result = get_repository_statuses::execute(vec![repo("api")], &runner).remove(0);

        assert_eq!(result.class(), StatusClass::Clean);
        assert_eq!(result.detail(), "✓");
        assert_eq!(
            result.repository(),
            &RepositoryStatus::Present {
                head: StatusHead::Branch {
                    name: branch_name("main"),
                    upstream: StatusUpstream::Tracking {
                        reference: GitRefName::try_new("origin/main").unwrap(),
                        ahead: CommitCount::default(),
                    },
                },
                changes: StatusChanges::Clean,
            }
        );
    }

    #[test]
    fn ahead_and_dirty_repo_classifies_as_pending_with_markers() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied(" M src/lib.rs\n?? new.txt\n"),
        ]);

        let result = get_repository_statuses::execute(vec![repo("api")], &runner).remove(0);

        assert_eq!(result.class(), StatusClass::Pending);
        assert_eq!(result.detail(), "⇡2 !?");
    }

    #[test]
    fn detached_and_missing_upstream_are_distinct_warning_states() {
        let detached = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied(""),
        ]);
        let missing_upstream = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("feat\n"),
            ScriptedGitClient::rejected("fatal: no upstream"),
            ScriptedGitClient::applied(""),
        ]);

        let detached = get_repository_statuses::execute(vec![repo("api")], &detached).remove(0);
        let missing =
            get_repository_statuses::execute(vec![repo("api")], &missing_upstream).remove(0);

        assert_eq!(detached.class(), StatusClass::Warn);
        assert_eq!(detached.detail(), "detached");
        assert_eq!(missing.class(), StatusClass::Warn);
        assert_eq!(missing.detail(), "no-upstream");
    }

    #[test]
    fn rejected_working_tree_read_is_not_reported_as_clean() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::rejected("status unavailable"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::rejected("status unavailable"),
        ]);

        let result = get_repository_statuses::execute(vec![repo("api")], &runner).remove(0);

        assert_eq!(result.class(), StatusClass::Warn);
        assert_eq!(result.detail(), "status-unavailable");
    }

    #[test]
    fn descendant_aware_statuses_inspect_children_first_and_preserve_output_order() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("child\n"),
            ScriptedGitClient::rejected("fatal: no upstream"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("parent\n"),
            ScriptedGitClient::rejected("fatal: no upstream"),
            ScriptedGitClient::applied(""),
        ]);
        let parent = repo("parent");
        let child = RepositoryTarget {
            label: crate::utils::project_name("parent/child"),
            path: crate::utils::repository_root("/repos/parent/child"),
        };

        let results = execute_with_known_descendants(vec![parent, child], &runner);

        assert_eq!(results[0].name().as_ref(), "parent");
        assert_eq!(results[1].name().as_ref(), "parent/child");
        assert!(matches!(
            results[0].repository(),
            RepositoryStatus::Present {
                head: StatusHead::Branch { name, .. },
                ..
            } if name.as_ref() == "parent"
        ));
        assert!(matches!(
            results[1].repository(),
            RepositoryStatus::Present {
                head: StatusHead::Branch { name, .. },
                ..
            } if name.as_ref() == "child"
        ));
    }
}
