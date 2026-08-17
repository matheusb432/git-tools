//! Shared results and preflight policy for project push and pull operations.

use gtl_models::{
    git::{BranchName, GitHead, RemoteName},
    paths::ProjectName,
    projects::ProjectRepository,
};

use crate::ports::GitClient;

/// One repository's push or pull outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSyncResult {
    pub name: ProjectName,
    pub branch: Option<BranchName>,
    pub status: SyncStatus,
    pub detail: String,
}

/// The outcome classification for one repo's push/pull. Replaces the former
/// stringly-typed status so [`classify_exit`] and every call site are checked
/// against the closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// Repo is not present on this machine; nothing was attempted.
    Skip,
    /// Already in sync with the remote.
    UpToDate,
    /// New commits were pushed.
    Pushed,
    /// Commits that a real run would push (dry-run).
    WouldPush,
    /// Local was fast-forwarded to match the remote.
    Pulled,
    /// A fast-forward a real run would perform (dry-run).
    WouldPull,
    /// A non-fatal problem (detached HEAD, no remote, missing branch).
    Warn,
    /// The operation failed.
    Fail,
}

/// The aggregate exit classification across every repo's result. Ported from
/// `push_pull.rs`'s retired `push_pull_exit_code` (`Usage` is dropped — push/pull
/// never produced it; both `Fail` and `Usage` mapped to the same CLI exit code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncExit {
    Clean,
    Warn,
    Fail,
}

pub(super) fn result(
    repo: &ProjectRepository,
    branch: Option<&BranchName>,
    status: SyncStatus,
    detail: &str,
) -> RepoSyncResult {
    RepoSyncResult {
        name: repo.name.clone(),
        branch: branch.cloned(),
        status,
        detail: detail.to_string(),
    }
}

pub(super) fn classify_exit(results: &[RepoSyncResult]) -> SyncExit {
    if results.iter().any(|r| r.status == SyncStatus::Fail) {
        return SyncExit::Fail;
    }
    if results.iter().any(|r| r.status == SyncStatus::Warn) {
        return SyncExit::Warn;
    }
    SyncExit::Clean
}

/// The shared present/branch/remote checks both `push_one` and `pull_one` run
/// before their operation-specific logic. `detached_detail` differs between the
/// two callers ("nothing to push" vs "nothing to pull onto").
pub(super) enum Preflight {
    Ready { branch: BranchName },
    Done(RepoSyncResult),
}

pub(super) fn preflight(
    git: &impl GitClient,
    repo: &ProjectRepository,
    detached_detail: &str,
) -> Preflight {
    if !git.repo_present(&repo.path) {
        return Preflight::Done(result(
            repo,
            None,
            SyncStatus::Skip,
            "not present on this machine",
        ));
    }
    let Ok(GitHead::Branch(branch)) = git.current_branch(&repo.path) else {
        return Preflight::Done(result(repo, None, SyncStatus::Warn, detached_detail));
    };
    match git.remote_url(&repo.path, &RemoteName::origin()) {
        Ok(Some(_)) => Preflight::Ready { branch },
        _ => Preflight::Done(result(
            repo,
            Some(&branch),
            SyncStatus::Warn,
            "no 'origin' remote",
        )),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::projects::ProjectRepository;

    use super::*;
    use crate::utils::ProjectGitScript;

    fn repo() -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name("repo"),
            path: crate::utils::repository_root("/repo"),
            remote: None,
        }
    }

    #[test]
    fn classify_exit_precedence_is_fail_then_warn_then_clean() {
        let clean = vec![
            result(&repo(), None, SyncStatus::Skip, ""),
            result(&repo(), None, SyncStatus::UpToDate, ""),
        ];
        let warn = vec![
            result(&repo(), None, SyncStatus::Warn, ""),
            result(&repo(), None, SyncStatus::UpToDate, ""),
        ];
        let fail = vec![
            result(&repo(), None, SyncStatus::Warn, ""),
            result(&repo(), None, SyncStatus::Fail, ""),
        ];
        assert_eq!(classify_exit(&clean), SyncExit::Clean);
        assert_eq!(classify_exit(&warn), SyncExit::Warn);
        assert_eq!(classify_exit(&fail), SyncExit::Fail);
    }

    #[tokio::test]
    async fn preflight_skips_a_repo_not_present_on_this_machine() {
        let remote = ProjectGitScript {
            present: false,
            ..Default::default()
        };
        let outcome = preflight(&remote.git_client(), &repo(), "detached");
        match outcome {
            Preflight::Done(r) => {
                assert_eq!(r.status, SyncStatus::Skip);
                assert_eq!(r.detail, "not present on this machine");
            }
            Preflight::Ready { .. } => panic!("expected Done"),
        }
    }

    #[tokio::test]
    async fn preflight_warns_on_detached_head() {
        let remote = ProjectGitScript {
            present: true,
            branch: "HEAD".into(),
            ..Default::default()
        };
        let outcome = preflight(
            &remote.git_client(),
            &repo(),
            "detached HEAD - nothing to push",
        );
        match outcome {
            Preflight::Done(r) => {
                assert_eq!(r.status, SyncStatus::Warn);
                assert_eq!(r.detail, "detached HEAD - nothing to push");
            }
            Preflight::Ready { .. } => panic!("expected Done"),
        }
    }

    #[tokio::test]
    async fn preflight_warns_when_no_origin_remote() {
        let remote = ProjectGitScript {
            present: true,
            branch: "main".into(),
            has_remote: false,
            ..Default::default()
        };
        let outcome = preflight(&remote.git_client(), &repo(), "detached");
        match outcome {
            Preflight::Done(r) => {
                assert_eq!(r.status, SyncStatus::Warn);
                assert_eq!(r.detail, "no 'origin' remote");
            }
            Preflight::Ready { .. } => panic!("expected Done"),
        }
    }

    #[tokio::test]
    async fn preflight_is_ready_when_every_check_passes() {
        let remote = ProjectGitScript {
            present: true,
            branch: "main".into(),
            has_remote: true,
            ..Default::default()
        };
        let outcome = preflight(&remote.git_client(), &repo(), "detached");
        match outcome {
            Preflight::Ready { branch } => assert_eq!(branch.as_ref(), "main"),
            Preflight::Done(r) => panic!("expected Ready, got {r:?}"),
        }
    }
}
