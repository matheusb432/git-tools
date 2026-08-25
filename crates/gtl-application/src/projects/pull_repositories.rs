//! The `pull_all` vertical slice: fan a fetch + fast-forward merge out across
//! every project repo, preserving the retired `push_pull.rs::pull_one`'s exact
//! status/detail semantics.

use futures_util::{StreamExt as _, stream};
use gtl_models::{
    git::{CommitCount, GitEffectMode, GitRange, GitRevision, RemoteName},
    projects::ProjectRepository,
};

use crate::{
    ports::{GitClient, GitEffect, ProjectClient, ProjectClientError},
    projects::remote_sync::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
};

const MAX_CONCURRENT_GIT_OPERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PullRepositoriesOk {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
}

#[derive(Debug, thiserror::Error)]
pub enum PullRepositoriesError {
    #[error(transparent)]
    ProjectClient(#[from] ProjectClientError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Pulls every project repository through the Git capability.
#[cqrsy::command]
pub async fn execute(
    mode: GitEffectMode,
    git: &impl GitClient,
    projects: &impl ProjectClient,
) -> Result<PullRepositoriesOk, PullRepositoriesError> {
    let repos = projects.list_projects().await?;
    let tasks = stream::iter(repos.into_iter().enumerate())
        .map(|(index, repo)| {
            let git = git.clone();
            async move {
                tokio::task::spawn_blocking(move || (index, pull_one(&git, &repo, mode)))
                    .await
                    .map_err(anyhow::Error::from)
            }
        })
        .buffer_unordered(MAX_CONCURRENT_GIT_OPERATIONS)
        .collect::<Vec<_>>()
        .await;
    let mut completed = tasks.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
    completed.sort_by_key(|(index, _)| *index);
    let results = completed
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Vec<_>>();
    let exit = remote_sync::classify_exit(&results);
    Ok(PullRepositoriesOk { results, exit })
}

fn pull_one(git: &impl GitClient, repo: &ProjectRepository, mode: GitEffectMode) -> RepoSyncResult {
    let branch = match remote_sync::preflight(git, repo, "detached HEAD - nothing to pull onto") {
        Preflight::Done(result) => return result,
        Preflight::Ready { branch } => branch,
    };

    let remote = RemoteName::origin();
    match git.fetch(&repo.path, &remote) {
        Ok(GitEffect::Applied(_)) => {}
        Ok(GitEffect::Rejected(detail)) => {
            let detail = format!("fetch failed: {detail}");
            return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail);
        }
        Err(error) => {
            return remote_sync::result(
                repo,
                Some(&branch),
                SyncStatus::Fail,
                &format!("fetch failed: {error}"),
            );
        }
    }

    let remote_branch = GitRevision::remote_branch(&remote, &branch);
    match git.revision_exists(&repo.path, &remote_branch) {
        Ok(true) => {}
        _ => {
            return remote_sync::result(
                repo,
                Some(&branch),
                SyncStatus::Warn,
                &format!("no '{branch}' branch on origin"),
            );
        }
    }

    let range = GitRange::three_dot(
        &GitRevision::remote_tracking(&remote, &branch),
        &GitRevision::from(&branch),
    );
    let Ok(Some(divergence)) = git.ahead_behind(&repo.path, &range) else {
        return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, "rev-list failed");
    };
    let behind = divergence.behind;
    let ahead = divergence.ahead;

    if behind == CommitCount::default() {
        let detail = if ahead == CommitCount::default() {
            "up to date".to_string()
        } else {
            format!("up to date (local ahead by {ahead} - push pending)")
        };
        return remote_sync::result(repo, Some(&branch), SyncStatus::UpToDate, &detail);
    }
    if ahead != CommitCount::default() {
        let detail = format!("diverged (ahead {ahead}, behind {behind}) - resolve manually");
        return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail);
    }
    if mode.is_dry_run() {
        let detail = format!("behind by {behind} - fast-forward");
        return remote_sync::result(repo, Some(&branch), SyncStatus::WouldPull, &detail);
    }

    let merge_ref = GitRevision::remote_tracking(&remote, &branch);
    match git.fast_forward(&repo.path, &merge_ref) {
        Ok(GitEffect::Applied(_)) => {
            let detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind.into_inner() == 1 { "" } else { "s" }
            );
            remote_sync::result(repo, Some(&branch), SyncStatus::Pulled, &detail)
        }
        Ok(GitEffect::Rejected(detail)) => {
            let detail = detail
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
                .map_or("ff merge failed", str::trim)
                .to_string();
            remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail)
        }
        Err(error) => {
            remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::projects::ProjectRepository;

    use super::*;
    use crate::{
        projects::pull_repositories,
        utils::{FakeProjectClient, ProjectGitScript, SyncOutput},
    };

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!("/repos/{name}")),
            remote: None,
        }
    }

    async fn execute_with(
        remote: ProjectGitScript,
        repos: Vec<ProjectRepository>,
        mode: GitEffectMode,
    ) -> Result<PullRepositoriesOk, PullRepositoriesError> {
        let projects = FakeProjectClient { repos, error: None };
        pull_repositories::execute(mode, &remote.git_client(), &projects).await
    }

    fn req() -> GitEffectMode {
        GitEffectMode::Apply
    }

    fn ready_remote() -> ProjectGitScript {
        ProjectGitScript {
            present: true,
            branch: "main".into(),
            has_remote: true,
            fetch_result: SyncOutput {
                success: true,
                combined: String::new(),
            },
            verify_ref: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn dry_run_reports_would_pull_without_merging() {
        let remote = ProjectGitScript {
            rev_list_left_right: (3, 0),
            ..ready_remote()
        };
        let request = GitEffectMode::DryRun;

        let response = execute_with(remote, vec![repo("a")], request)
            .await
            .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::WouldPull);
        assert_eq!(response.results[0].detail, "behind by 3 - fast-forward");
    }

    #[tokio::test]
    async fn diverged_repos_fail_without_merging() {
        let response = execute_with(
            ProjectGitScript {
                rev_list_left_right: (2, 1),
                ..ready_remote()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Fail);
        assert_eq!(
            response.results[0].detail,
            "diverged (ahead 1, behind 2) - resolve manually"
        );
        assert_eq!(response.exit, SyncExit::Fail);
    }

    #[tokio::test]
    async fn already_up_to_date_with_local_ahead_reports_push_pending() {
        let response = execute_with(
            ProjectGitScript {
                rev_list_left_right: (0, 2),
                ..ready_remote()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::UpToDate);
        assert_eq!(
            response.results[0].detail,
            "up to date (local ahead by 2 - push pending)"
        );
    }

    #[tokio::test]
    async fn a_fast_forward_merge_succeeds() {
        let response = execute_with(
            ProjectGitScript {
                rev_list_left_right: (1, 0),
                merge_result: SyncOutput {
                    success: true,
                    combined: String::new(),
                },
                ..ready_remote()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Pulled);
        assert_eq!(response.results[0].detail, "fast-forwarded 1 commit");
    }

    #[tokio::test]
    async fn a_fetch_failure_is_reported_as_fail() {
        let response = execute_with(
            ProjectGitScript {
                fetch_result: SyncOutput {
                    success: false,
                    combined: "fatal: unable to access origin\n".into(),
                },
                ..ready_remote()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Fail);
        assert_eq!(
            response.results[0].detail,
            "fetch failed: fatal: unable to access origin"
        );
    }

    #[tokio::test]
    async fn a_missing_remote_branch_is_a_warning() {
        let response = execute_with(
            ProjectGitScript {
                verify_ref: false,
                ..ready_remote()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Warn);
        assert_eq!(response.results[0].detail, "no 'main' branch on origin");
    }
}
