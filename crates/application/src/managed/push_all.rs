//! The `push_all` vertical slice: fan a `git push` out across every managed repo,
//! preserving the retired `push_pull.rs::push_one`'s exact status/detail semantics.

use std::path::PathBuf;

use domain::managed::ManagedRepo;

use crate::{
    managed::service::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
    ports::{Clock, ManagedManifest, PushLedger, RemoteSync},
};

#[derive(Debug, Clone, PartialEq)]
pub struct PushAll {
    pub repos_file: PathBuf,
    pub home_dir: PathBuf,
    pub dry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PushAllResponse {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
}

#[derive(Debug, thiserror::Error)]
pub enum PushAllError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Pushes every managed repository through the remote-sync ports.
#[cqrsy::handler(command)]
pub async fn execute(
    req: PushAll,
    remote: &impl RemoteSync,
    manifest: &impl ManagedManifest,
    ledger: &impl PushLedger,
    clock: &impl Clock,
) -> Result<PushAllResponse, PushAllError> {
    let repos = manifest.load(&req.repos_file, &req.home_dir).await?;
    let results = futures_util::future::join_all(
        repos
            .iter()
            .map(|repo| push_one(remote, ledger, clock, repo, req.dry)),
    )
    .await;
    let exit = service::classify_exit(&results);
    Ok(PushAllResponse { results, exit })
}

async fn push_one(
    remote: &impl RemoteSync,
    ledger: &impl PushLedger,
    clock: &impl Clock,
    repo: &ManagedRepo,
    dry: bool,
) -> RepoSyncResult {
    let branch = match service::preflight(remote, repo, "detached HEAD - nothing to push").await {
        Preflight::Done(result) => return result,
        Preflight::Ready { branch } => branch,
    };

    let ahead = synced_ahead(remote, repo).await;
    if ahead == Some(0) {
        ledger.record(&repo.name, 0, &clock.now_iso()).await;
        return service::result(
            repo,
            &branch,
            SyncStatus::UpToDate,
            "up to date (already synced)",
        );
    }

    let outcome = match remote.push(&repo.path, "origin", &branch, dry).await {
        Ok(outcome) => outcome,
        Err(error) => return service::result(repo, &branch, SyncStatus::Fail, &error.to_string()),
    };
    ledger
        .record(&repo.name, ahead.unwrap_or(0), &clock.now_iso())
        .await;

    if !outcome.success {
        return service::result(
            repo,
            &branch,
            SyncStatus::Fail,
            &push_failure_detail(&outcome.combined),
        );
    }
    if outcome.combined.contains("Everything up-to-date") {
        return service::result(repo, &branch, SyncStatus::UpToDate, "up to date");
    }
    let status = if dry {
        SyncStatus::WouldPush
    } else {
        SyncStatus::Pushed
    };
    let detail = service::last_non_empty_line(&outcome.combined).unwrap_or("up to date");
    service::result(repo, &branch, status, detail)
}

/// `Some(ahead)` when an upstream exists (mirrors the CLI's retired
/// `status::upstream_ahead`, including its "a rev-list failure counts as 0"
/// quirk); `None` when there is no upstream at all, so push always proceeds.
async fn synced_ahead(remote: &impl RemoteSync, repo: &ManagedRepo) -> Option<usize> {
    match remote.upstream_ref(&repo.path).await {
        Ok(Some(_)) => Some(
            remote
                .rev_list_count(&repo.path, "@{u}..HEAD")
                .await
                .unwrap_or(0),
        ),
        _ => None,
    }
}

fn push_failure_detail(output: &str) -> String {
    output
        .lines()
        .find(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with('!')
                || trimmed.starts_with("error:")
                || trimmed.starts_with("fatal:")
        })
        .map(str::trim)
        .or_else(|| service::last_non_empty_line(output))
        .unwrap_or("push failed")
        .to_string()
}

#[cfg(test)]
mod tests {
    use domain::managed::ManagedRepo;

    use super::*;
    use crate::testing::{FakeManagedManifest, FakePushLedger, FakeRemoteSync, FixedClock};

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
        request: PushAll,
    ) -> Result<PushAllResponse, PushAllError> {
        let manifest = FakeManagedManifest { repos, error: None };
        execute(
            request,
            &remote,
            &manifest,
            &FakePushLedger::default(),
            &FixedClock("2026-07-03T00:00:00Z".into()),
        )
        .await
    }

    fn req() -> PushAll {
        PushAll {
            repos_file: "/repos.toml".into(),
            home_dir: "/home".into(),
            dry: false,
        }
    }

    #[tokio::test]
    async fn skips_a_repo_already_synced_with_its_upstream() {
        let response = execute_with(
            FakeRemoteSync {
                present: true,
                branch: "main".into(),
                has_remote: true,
                upstream: Some("origin/main".into()),
                rev_list_count: 0,
                ..Default::default()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("push succeeds");

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].status, SyncStatus::UpToDate);
        assert_eq!(response.results[0].detail, "up to date (already synced)");
        assert_eq!(response.exit, SyncExit::Clean);
    }

    #[tokio::test]
    async fn pushes_a_repo_with_unpushed_commits() {
        let response = execute_with(
            FakeRemoteSync {
                present: true,
                branch: "main".into(),
                has_remote: true,
                upstream: Some("origin/main".into()),
                rev_list_count: 2,
                push_result: crate::ports::SyncOutput {
                    success: true,
                    combined: "\n   abc..def  main -> main\n".into(),
                },
                ..Default::default()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("push succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Pushed);
        assert_eq!(response.results[0].detail, "abc..def  main -> main");
        assert_eq!(response.exit, SyncExit::Clean);
    }

    #[tokio::test]
    async fn dry_run_reports_would_push_without_treating_it_as_pushed() {
        let remote = FakeRemoteSync {
            present: true,
            branch: "main".into(),
            has_remote: true,
            rev_list_count: 1,
            push_result: crate::ports::SyncOutput {
                success: true,
                combined: "would push".into(),
            },
            ..Default::default()
        };
        let mut request = req();
        request.dry = true;

        let response = execute_with(remote, vec![repo("a")], request)
            .await
            .expect("push succeeds");

        assert_eq!(response.results[0].status, SyncStatus::WouldPush);
    }

    #[tokio::test]
    async fn a_rejected_push_reports_fail_and_the_exit_precedence_wins() {
        let response = execute_with(
            FakeRemoteSync {
                present: true,
                branch: "main".into(),
                has_remote: true,
                rev_list_count: 1,
                push_result: crate::ports::SyncOutput {
                    success: false,
                    combined: "! [rejected]  main -> main (fetch first)\nerror: failed to push"
                        .into(),
                },
                ..Default::default()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("push succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Fail);
        assert_eq!(
            response.results[0].detail,
            "! [rejected]  main -> main (fetch first)"
        );
        assert_eq!(response.exit, SyncExit::Fail);
    }

    #[tokio::test]
    async fn a_detached_head_repo_is_reported_as_a_warning_without_touching_the_remote() {
        let response = execute_with(
            FakeRemoteSync {
                present: true,
                branch: "HEAD".into(),
                ..Default::default()
            },
            vec![repo("a")],
            req(),
        )
        .await
        .expect("push succeeds");

        assert_eq!(response.results[0].status, SyncStatus::Warn);
        assert_eq!(
            response.results[0].detail,
            "detached HEAD - nothing to push"
        );
        assert_eq!(response.exit, SyncExit::Warn);
    }

    #[tokio::test]
    async fn fanning_out_over_multiple_repos_preserves_each_repos_identity() {
        let response = execute_with(
            FakeRemoteSync {
                present: false,
                ..Default::default()
            },
            vec![repo("a"), repo("b"), repo("c")],
            req(),
        )
        .await
        .expect("push succeeds");

        assert_eq!(response.results.len(), 3);
        assert_eq!(
            response
                .results
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[tokio::test]
    async fn a_manifest_load_failure_propagates_as_an_error() {
        let error = execute(
            req(),
            &FakeRemoteSync::default(),
            &FakeManagedManifest {
                repos: Vec::new(),
                error: Some("boom".into()),
            },
            &FakePushLedger::default(),
            &FixedClock("2026-07-03T00:00:00Z".into()),
        )
        .await
        .expect_err("manifest error propagates");
        assert!(format!("{error:#}").contains("boom"));
    }
}
