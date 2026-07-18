//! The `pull_all` vertical slice: fan a fetch + fast-forward merge out across
//! every managed repo, preserving the retired `push_pull.rs::pull_one`'s exact
//! status/detail semantics.

use std::path::PathBuf;

use domain::managed::ManagedRepo;

use crate::{
    managed::service::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
    ports::{ManagedManifest, RemoteSync},
};

#[derive(Debug, Clone, PartialEq)]
pub struct PullAll {
    pub repos_file: PathBuf,
    pub home_dir: PathBuf,
    pub dry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PullAllResponse {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
}

#[derive(Debug, thiserror::Error)]
pub enum PullAllError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Pulls every managed repository through the remote-sync ports.
#[cqrsy::command]
pub async fn execute(
    req: PullAll,
    remote: &impl RemoteSync,
    manifest: &impl ManagedManifest,
) -> Result<PullAllResponse, PullAllError> {
    let repos = manifest.load(&req.repos_file, &req.home_dir).await?;
    let results =
        futures_util::future::join_all(repos.iter().map(|repo| pull_one(remote, repo, req.dry)))
            .await;
    let exit = service::classify_exit(&results);
    Ok(PullAllResponse { results, exit })
}

async fn pull_one(remote: &impl RemoteSync, repo: &ManagedRepo, dry: bool) -> RepoSyncResult {
    let branch =
        match service::preflight(remote, repo, "detached HEAD - nothing to pull onto").await {
            Preflight::Done(result) => return result,
            Preflight::Ready { branch } => branch,
        };

    match remote.fetch(&repo.path, "origin").await {
        Ok(outcome) if outcome.success => {}
        Ok(outcome) => {
            let detail = format!(
                "fetch failed: {}",
                crate::shared::git::last_non_empty_line(&outcome.combined)
                    .unwrap_or("fetch failed")
            );
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
    match remote.verify_ref(&repo.path, &remote_branch).await {
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
    let Ok((behind, ahead)) = remote.rev_list_left_right(&repo.path, &range).await else {
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
    match remote.merge_ff_only(&repo.path, &merge_ref).await {
        Ok(outcome) if outcome.success => {
            let detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind == 1 { "" } else { "s" }
            );
            service::result(repo, &branch, SyncStatus::Pulled, &detail)
        }
        Ok(outcome) => {
            let detail = outcome
                .combined
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
    use domain::managed::ManagedRepo;

    use super::*;
    use crate::{
        ports::SyncOutput,
        testing::{FakeManagedManifest, FakeRemoteSync},
    };

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: format!("/repos/{name}").into(),
            remote: String::new(),
        }
    }

    async fn execute_with(
        remote: FakeRemoteSync,
        repos: Vec<ManagedRepo>,
        request: PullAll,
    ) -> Result<PullAllResponse, PullAllError> {
        let manifest = FakeManagedManifest { repos, error: None };
        execute(request, &remote, &manifest).await
    }

    fn req() -> PullAll {
        PullAll {
            repos_file: "/repos.toml".into(),
            home_dir: "/home".into(),
            dry: false,
        }
    }

    fn ready_remote() -> FakeRemoteSync {
        FakeRemoteSync {
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
        let remote = FakeRemoteSync {
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
            FakeRemoteSync {
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
            FakeRemoteSync {
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
            FakeRemoteSync {
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
            FakeRemoteSync {
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
            FakeRemoteSync {
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
