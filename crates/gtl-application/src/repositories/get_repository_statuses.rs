//! Classifies repositories from local head, upstream, and working-tree facts.

use gtl_models::{
    git::{CommitCount, GitHead, GitRange},
    paths::RepositoryRoot,
    repository::{
        PathCount,
        status::{StatusChanges, StatusHead, StatusResult, StatusUpstream},
        traversal::RepositoryTarget,
        working_tree::DirtyState,
    },
};

use crate::ports::GitClient;

/// Classifies every supplied repository in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRepositoryStatuses {
    pub repos: Vec<RepositoryTarget>,
}

/// Classifies each repository from local refs without fetching.
///
/// Transport and Git rejections become explicit unavailable facts so an unreadable repository is
/// never reported as clean.
#[cqrsy::query]
pub fn execute(query: GetRepositoryStatuses, git: &impl GitClient) -> Vec<StatusResult> {
    let GetRepositoryStatuses { repos } = query;
    repos.iter().map(|repo| status_one(git, repo)).collect()
}

fn status_one(git: &impl GitClient, repo: &RepositoryTarget) -> StatusResult {
    if !git.repo_present(&repo.path) {
        return StatusResult::absent(repo.label.clone());
    }

    StatusResult::present(
        repo.label.clone(),
        status_head(git, &repo.path),
        status_changes(git, &repo.path),
    )
}

fn status_head(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusHead {
    match git.current_branch(repo_path) {
        Ok(GitHead::Detached) => StatusHead::Detached,
        Ok(GitHead::Branch(name)) => StatusHead::Branch {
            name,
            upstream: upstream(git, repo_path),
        },
        Err(_) => StatusHead::Unavailable,
    }
}

fn upstream(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusUpstream {
    let Ok(crate::ports::GitEffect::Applied(reference)) = git.upstream(repo_path) else {
        return StatusUpstream::Missing;
    };
    StatusUpstream::Tracking {
        reference,
        ahead: ahead_count(git, repo_path),
    }
}

fn ahead_count(git: &impl GitClient, repo_path: &RepositoryRoot) -> CommitCount {
    git.commit_count(repo_path, &GitRange::upstream_to_head())
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn status_changes(git: &impl GitClient, repo_path: &RepositoryRoot) -> StatusChanges {
    match working_tree_state(git, repo_path) {
        DirtyState::Clean => StatusChanges::Clean,
        DirtyState::Dirty(files) => {
            let untracked = files
                .as_slice()
                .iter()
                .filter(|file| file.is_untracked())
                .count();
            let tracked = files.len().saturating_sub(untracked);
            StatusChanges::from_counts(PathCount::from_len(tracked), PathCount::from_len(untracked))
        }
        DirtyState::Unavailable { .. } | DirtyState::Absent => StatusChanges::Unavailable,
    }
}

fn working_tree_state(git: &impl GitClient, repo_path: &RepositoryRoot) -> DirtyState {
    super::working_tree::read(git, repo_path).unwrap_or_else(|error| DirtyState::Unavailable {
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::GitRefName,
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

        let results = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &runner,
        );

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

        let result = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &runner,
        )
        .remove(0);

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

        let result = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &runner,
        )
        .remove(0);

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

        let detached = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &detached,
        )
        .remove(0);
        let missing = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &missing_upstream,
        )
        .remove(0);

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
        ]);

        let result = get_repository_statuses::execute(
            GetRepositoryStatuses {
                repos: vec![repo("api")],
            },
            &runner,
        )
        .remove(0);

        assert_eq!(result.class(), StatusClass::Warn);
        assert_eq!(result.detail(), "status-unavailable");
    }
}
