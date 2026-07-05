//! The `pull_all` vertical slice: fan a fetch + fast-forward merge out across
//! every managed repo, preserving the retired `push_pull.rs::pull_one`'s exact
//! status/detail semantics.

use std::path::PathBuf;

use cqrsy::Handler;
use domain::managed::ManagedRepo;

use crate::{
    managed::service::{self, Preflight, RepoSyncResult, SyncExit},
    ports::{ManagedManifest, RemoteSync},
};

#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = PullAllResponse, err = PullAllError)]
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

#[derive(Clone)]
pub struct PullAllHandler<RS: RemoteSync, ML: ManagedManifest> {
    pub remote: RS,
    pub manifest: ML,
}

impl<RS: RemoteSync, ML: ManagedManifest> Handler<PullAll> for PullAllHandler<RS, ML> {
    async fn handle(&self, req: PullAll) -> Result<PullAllResponse, PullAllError> {
        let repos = self.manifest.load(&req.repos_file, &req.home_dir).await?;
        let results = futures_util::future::join_all(
            repos
                .iter()
                .map(|repo| pull_one(&self.remote, repo, req.dry)),
        )
        .await;
        let exit = service::classify_exit(&results);
        Ok(PullAllResponse { results, exit })
    }
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
                service::last_non_empty_line(&outcome.combined).unwrap_or("fetch failed")
            );
            return service::result(repo, &branch, "fail", &detail);
        }
        Err(error) => {
            return service::result(repo, &branch, "fail", &format!("fetch failed: {error}"));
        }
    }

    let remote_branch = format!("refs/remotes/origin/{branch}");
    match remote.verify_ref(&repo.path, &remote_branch).await {
        Ok(true) => {}
        _ => {
            return service::result(
                repo,
                &branch,
                "warn",
                &format!("no '{branch}' branch on origin"),
            );
        }
    }

    let range = format!("origin/{branch}...{branch}");
    let (behind, ahead) = match remote.rev_list_left_right(&repo.path, &range).await {
        Ok(counts) => counts,
        Err(_) => return service::result(repo, &branch, "fail", "rev-list failed"),
    };

    if behind == 0 {
        let detail = if ahead > 0 {
            format!("up to date (local ahead by {ahead} - push pending)")
        } else {
            "up to date".to_string()
        };
        return service::result(repo, &branch, "up-to-date", &detail);
    }
    if ahead > 0 {
        let detail = format!("diverged (ahead {ahead}, behind {behind}) - resolve manually");
        return service::result(repo, &branch, "fail", &detail);
    }
    if dry {
        let detail = format!("behind by {behind} - fast-forward");
        return service::result(repo, &branch, "would-pull", &detail);
    }

    let merge_ref = format!("origin/{branch}");
    match remote.merge_ff_only(&repo.path, &merge_ref).await {
        Ok(outcome) if outcome.success => {
            let detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind == 1 { "" } else { "s" }
            );
            service::result(repo, &branch, "pulled", &detail)
        }
        Ok(outcome) => {
            let detail = outcome
                .combined
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
                .map(str::trim)
                .unwrap_or("ff merge failed")
                .to_string();
            service::result(repo, &branch, "fail", &detail)
        }
        Err(error) => service::result(repo, &branch, "fail", &error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;
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

    fn handler_with(
        remote: FakeRemoteSync,
        repos: Vec<ManagedRepo>,
    ) -> PullAllHandler<FakeRemoteSync, FakeManagedManifest> {
        PullAllHandler {
            remote,
            manifest: FakeManagedManifest { repos, error: None },
        }
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

    #[test]
    fn dry_run_reports_would_pull_without_merging() {
        let handler = handler_with(
            FakeRemoteSync {
                rev_list_left_right: (3, 0),
                ..ready_remote()
            },
            vec![repo("a")],
        );
        let mut request = req();
        request.dry = true;

        let response = send_now(&(), &handler, request).expect("pull succeeds");

        assert_eq!(response.results[0].status, "would-pull");
        assert_eq!(response.results[0].detail, "behind by 3 - fast-forward");
    }

    #[test]
    fn diverged_repos_fail_without_merging() {
        let handler = handler_with(
            FakeRemoteSync {
                rev_list_left_right: (2, 1),
                ..ready_remote()
            },
            vec![repo("a")],
        );

        let response = send_now(&(), &handler, req()).expect("pull succeeds");

        assert_eq!(response.results[0].status, "fail");
        assert_eq!(
            response.results[0].detail,
            "diverged (ahead 1, behind 2) - resolve manually"
        );
        assert_eq!(response.exit, SyncExit::Fail);
    }

    #[test]
    fn already_up_to_date_with_local_ahead_reports_push_pending() {
        let handler = handler_with(
            FakeRemoteSync {
                rev_list_left_right: (0, 2),
                ..ready_remote()
            },
            vec![repo("a")],
        );

        let response = send_now(&(), &handler, req()).expect("pull succeeds");

        assert_eq!(response.results[0].status, "up-to-date");
        assert_eq!(
            response.results[0].detail,
            "up to date (local ahead by 2 - push pending)"
        );
    }

    #[test]
    fn a_fast_forward_merge_succeeds() {
        let handler = handler_with(
            FakeRemoteSync {
                rev_list_left_right: (1, 0),
                merge_result: SyncOutput {
                    success: true,
                    combined: String::new(),
                },
                ..ready_remote()
            },
            vec![repo("a")],
        );

        let response = send_now(&(), &handler, req()).expect("pull succeeds");

        assert_eq!(response.results[0].status, "pulled");
        assert_eq!(response.results[0].detail, "fast-forwarded 1 commit");
    }

    #[test]
    fn a_fetch_failure_is_reported_as_fail() {
        let handler = handler_with(
            FakeRemoteSync {
                fetch_result: SyncOutput {
                    success: false,
                    combined: "fatal: unable to access origin\n".into(),
                },
                ..ready_remote()
            },
            vec![repo("a")],
        );

        let response = send_now(&(), &handler, req()).expect("pull succeeds");

        assert_eq!(response.results[0].status, "fail");
        assert_eq!(
            response.results[0].detail,
            "fetch failed: fatal: unable to access origin"
        );
    }

    #[test]
    fn a_missing_remote_branch_is_a_warning() {
        let handler = handler_with(
            FakeRemoteSync {
                verify_ref: false,
                ..ready_remote()
            },
            vec![repo("a")],
        );

        let response = send_now(&(), &handler, req()).expect("pull succeeds");

        assert_eq!(response.results[0].status, "warn");
        assert_eq!(response.results[0].detail, "no 'main' branch on origin");
    }
}
