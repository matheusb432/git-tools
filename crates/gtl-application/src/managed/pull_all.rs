//! The `pull_all` vertical slice: fan a fetch + fast-forward merge out across
//! every managed repo, preserving the retired `push_pull.rs::pull_one`'s exact
//! status/detail semantics.

use futures_util::{StreamExt as _, stream};
use gtl_models::managed::ManagedRepo;

use crate::{
    managed::logic::service::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
    ports::{GitClient, GitEffect, ProjectClient, ProjectClientError},
};

const MAX_CONCURRENT_GIT_OPERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PullAll {
    pub dry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PullAllOk {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
}

#[derive(Debug, thiserror::Error)]
pub enum PullAllError {
    #[error(transparent)]
    ProjectClient(#[from] ProjectClientError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Pulls every managed repository through the Git capability.
#[cqrsy::command]
pub async fn execute(
    req: PullAll,
    git: &impl GitClient,
    projects: &impl ProjectClient,
) -> Result<PullAllOk, PullAllError> {
    let repos = projects.list_projects().await?;
    let dry = req.dry;
    let tasks = stream::iter(repos.into_iter().enumerate())
        .map(|(index, repo)| {
            let git = git.clone();
            async move {
                tokio::task::spawn_blocking(move || (index, pull_one(&git, &repo, dry)))
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
    let exit = service::classify_exit(&results);
    Ok(PullAllOk { results, exit })
}

fn pull_one(git: &impl GitClient, repo: &ManagedRepo, dry: bool) -> RepoSyncResult {
    let branch = match service::preflight(git, repo, "detached HEAD - nothing to pull onto") {
        Preflight::Done(result) => return result,
        Preflight::Ready { branch } => branch,
    };

    match git.fetch(&repo.path, "origin") {
        Ok(GitEffect::Applied(_)) => {}
        Ok(GitEffect::Rejected(detail)) => {
            let detail = format!("fetch failed: {detail}");
            return service::result(repo, &branch, SyncStatus::Fail, &detail);
        }
        Err(error) => {
            return service::result(
                repo,
                &branch,
                SyncStatus::Fail,
                &format!("fetch failed: {error}"),
            );
        }
    }

    let remote_branch = format!("refs/remotes/origin/{branch}");
    match git.revision_exists(&repo.path, &remote_branch) {
        Ok(true) => {}
        _ => {
            return service::result(
                repo,
                &branch,
                SyncStatus::Warn,
                &format!("no '{branch}' branch on origin"),
            );
        }
    }

    let range = format!("origin/{branch}...{branch}");
    let Ok(Some((behind, ahead))) = git.ahead_behind(&repo.path, &range) else {
        return service::result(repo, &branch, SyncStatus::Fail, "rev-list failed");
    };

    if behind == 0 {
        let detail = if ahead > 0 {
            format!("up to date (local ahead by {ahead} - push pending)")
        } else {
            "up to date".to_string()
        };
        return service::result(repo, &branch, SyncStatus::UpToDate, &detail);
    }
    if ahead > 0 {
        let detail = format!("diverged (ahead {ahead}, behind {behind}) - resolve manually");
        return service::result(repo, &branch, SyncStatus::Fail, &detail);
    }
    if dry {
        let detail = format!("behind by {behind} - fast-forward");
        return service::result(repo, &branch, SyncStatus::WouldPull, &detail);
    }

    let merge_ref = format!("origin/{branch}");
    match git.fast_forward(&repo.path, &merge_ref) {
        Ok(GitEffect::Applied(_)) => {
            let detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind == 1 { "" } else { "s" }
            );
            service::result(repo, &branch, SyncStatus::Pulled, &detail)
        }
        Ok(GitEffect::Rejected(detail)) => {
            let detail = detail
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
                .map_or("ff merge failed", str::trim)
                .to_string();
            service::result(repo, &branch, SyncStatus::Fail, &detail)
        }
        Err(error) => service::result(repo, &branch, SyncStatus::Fail, &error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::managed::ManagedRepo;

    use super::*;
    use crate::testing::{FakeProjectClient, ManagedGitScript, SyncOutput};

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: format!("/repos/{name}").into(),
            remote: String::new(),
        }
    }

    async fn execute_with(
        remote: ManagedGitScript,
        repos: Vec<ManagedRepo>,
        request: PullAll,
    ) -> Result<PullAllOk, PullAllError> {
        let projects = FakeProjectClient { repos, error: None };
        execute(request, &remote.git_client(), &projects).await
    }

    fn req() -> PullAll {
        PullAll { dry: false }
    }

    fn ready_remote() -> ManagedGitScript {
        ManagedGitScript {
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
        let remote = ManagedGitScript {
            rev_list_left_right: (3, 0),
            ..ready_remote()
        };
        let mut request = req();
        request.dry = true;

        let response = execute_with(remote, vec![repo("a")], request)
            .await
            .expect("pull succeeds");

        assert_eq!(response.results[0].status, SyncStatus::WouldPull);
        assert_eq!(response.results[0].detail, "behind by 3 - fast-forward");
    }

    #[tokio::test]
    async fn diverged_repos_fail_without_merging() {
        let response = execute_with(
            ManagedGitScript {
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
            ManagedGitScript {
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
            ManagedGitScript {
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
            ManagedGitScript {
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
            ManagedGitScript {
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
