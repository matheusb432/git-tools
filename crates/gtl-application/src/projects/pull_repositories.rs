use futures_util::{StreamExt as _, stream};
use gtl_models::{failure::ErrorMeta, git::GitEffectMode};

use crate::{
    ports::{GitClient, ProjectClient, ProjectClientError},
    projects::remote_sync::{self, RepoSyncResult, SyncExit},
    repositories::pull_repository,
};

const MAX_CONCURRENT_GIT_OPERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PullRepositoriesOk {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PullRepositoriesError {
    #[error(transparent)]
    #[meta(transparent)]
    ProjectClient(#[from] ProjectClientError),
    #[error(transparent)]
    #[meta(private(Internal))]
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
                tokio::task::spawn_blocking(move || {
                    (index, pull_repository::pull_one(&git, &repo, mode))
                })
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

#[cfg(test)]
mod tests {
    use gtl_models::projects::ProjectRepository;

    use super::*;
    use crate::{
        projects::{SyncStatus, pull_repositories},
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
            .unwrap();

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
        .unwrap();

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
        .unwrap();

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
        .unwrap();

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
        .unwrap();

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
        .unwrap();

        assert_eq!(response.results[0].status, SyncStatus::Warn);
        assert_eq!(response.results[0].detail, "no 'main' branch on origin");
    }
}
