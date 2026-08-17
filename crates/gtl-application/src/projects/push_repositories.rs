//! The `push_all` vertical slice: fan a `git push` out across every project repo,
//! preserving the retired `push_pull.rs::push_one`'s exact status/detail semantics.

use futures_util::{StreamExt as _, stream};
use gtl_models::{
    git::{CommitCount, GitEffectMode, GitRange, RemoteName},
    projects::{ProjectRepository, push_ledger::PushLedger},
};

use crate::{
    ports::{Clock, GitClient, GitEffect, ProjectClient, ProjectClientError},
    projects::remote_sync::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
};

const MAX_CONCURRENT_GIT_OPERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PushRepositories {
    pub mode: GitEffectMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PushRepositoriesOk {
    pub results: Vec<RepoSyncResult>,
    pub exit: SyncExit,
    pub ledger: PushLedger,
}

#[derive(Debug, thiserror::Error)]
pub enum PushRepositoriesError {
    #[error(transparent)]
    ProjectClient(#[from] ProjectClientError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Pushes every project repository through the Git capability.
#[cqrsy::command]
pub async fn execute(
    req: PushRepositories,
    git: &impl GitClient,
    projects: &impl ProjectClient,
    clock: &impl Clock,
) -> Result<PushRepositoriesOk, PushRepositoriesError> {
    let repos = projects.list_projects().await?;
    let mode = req.mode;
    let tasks = stream::iter(repos.into_iter().enumerate())
        .map(|(index, repo)| {
            let git = git.clone();
            async move {
                tokio::task::spawn_blocking(move || {
                    let outcome = push_one(&git, &repo, mode);
                    (index, repo.name, outcome)
                })
                .await
                .map_err(anyhow::Error::from)
            }
        })
        .buffer_unordered(MAX_CONCURRENT_GIT_OPERATIONS)
        .collect::<Vec<_>>()
        .await;
    let mut completed = tasks.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
    completed.sort_by_key(|(index, _, _)| *index);
    let mut results = Vec::with_capacity(completed.len());
    let mut ledger = PushLedger::default();
    for (_, repo_name, outcome) in completed {
        if let Some(ahead) = outcome.ledger_ahead {
            ledger.record(repo_name, ahead, clock.now().map_err(anyhow::Error::from)?);
        }
        results.push(outcome.result);
    }
    let exit = remote_sync::classify_exit(&results);
    Ok(PushRepositoriesOk {
        results,
        exit,
        ledger,
    })
}

struct PushOneOutcome {
    result: RepoSyncResult,
    ledger_ahead: Option<CommitCount>,
}

fn push_one(git: &impl GitClient, repo: &ProjectRepository, mode: GitEffectMode) -> PushOneOutcome {
    let branch = match remote_sync::preflight(git, repo, "detached HEAD - nothing to push") {
        Preflight::Done(result) => {
            return PushOneOutcome {
                result,
                ledger_ahead: None,
            };
        }
        Preflight::Ready { branch } => branch,
    };

    let ahead = synced_ahead(git, repo);
    if ahead.is_some_and(|count| count == CommitCount::default()) {
        return PushOneOutcome {
            result: remote_sync::result(
                repo,
                Some(&branch),
                SyncStatus::UpToDate,
                "up to date (already synced)",
            ),
            ledger_ahead: Some(CommitCount::default()),
        };
    }

    let outcome = match git.push_branch(&repo.path, &RemoteName::origin(), &branch, mode) {
        Ok(outcome) => outcome,
        Err(error) => {
            return PushOneOutcome {
                result: remote_sync::result(
                    repo,
                    Some(&branch),
                    SyncStatus::Fail,
                    &error.to_string(),
                ),
                ledger_ahead: None,
            };
        }
    };
    let result = match outcome {
        GitEffect::Rejected(detail) => remote_sync::result(
            repo,
            Some(&branch),
            SyncStatus::Fail,
            &push_failure_detail(&detail),
        ),
        GitEffect::Applied(crate::ports::GitPushReceipt::UpToDate { .. }) => {
            remote_sync::result(repo, Some(&branch), SyncStatus::UpToDate, "up to date")
        }
        GitEffect::Applied(receipt @ crate::ports::GitPushReceipt::Updated { .. }) => {
            let status = if mode.is_dry_run() {
                SyncStatus::WouldPush
            } else {
                SyncStatus::Pushed
            };
            remote_sync::result(repo, Some(&branch), status, receipt.detail())
        }
    };
    PushOneOutcome {
        result,
        ledger_ahead: Some(ahead.unwrap_or_default()),
    }
}

/// `Some(ahead)` when an upstream exists (mirrors the CLI's retired
/// `status::upstream_ahead`, including its "a rev-list failure counts as 0"
/// quirk); `None` when there is no upstream at all, so push always proceeds.
fn synced_ahead(git: &impl GitClient, repo: &ProjectRepository) -> Option<CommitCount> {
    match git.upstream(&repo.path) {
        Ok(GitEffect::Applied(_)) => Some(
            git.commit_count(&repo.path, &GitRange::upstream_to_head())
                .ok()
                .flatten()
                .unwrap_or_default(),
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
        .or_else(|| last_non_empty_line(output))
        .unwrap_or("push failed")
        .to_string()
}

fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use gtl_models::projects::{ProjectRepository, push_ledger::PushLedgerEntry};

    use super::*;
    use crate::{
        projects::push_repositories,
        utils::{FakeProjectClient, FixedClock, ProjectGitScript, SyncOutput},
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
        request: PushRepositories,
    ) -> Result<PushRepositoriesOk, PushRepositoriesError> {
        let projects = FakeProjectClient { repos, error: None };
        let git = remote.git_client();
        push_repositories::execute(
            request,
            &git,
            &projects,
            &FixedClock::from_raw("2026-07-03T00:00:00Z"),
        )
        .await
    }

    fn req() -> PushRepositories {
        PushRepositories {
            mode: GitEffectMode::Apply,
        }
    }

    #[tokio::test]
    async fn skips_a_repo_already_synced_with_its_upstream() {
        let response = execute_with(
            ProjectGitScript {
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
        assert_eq!(
            response.ledger.entries(),
            &[PushLedgerEntry {
                repository_name: crate::utils::project_name("a"),
                ahead: CommitCount::default(),
                checked_at: gtl_models::timestamps::MachineTimestamp::try_from(
                    "2026-07-03T00:00:00Z",
                )
                .expect("fixture ledger timestamp is valid"),
            }]
        );
    }

    #[tokio::test]
    async fn pushes_a_repo_with_unpushed_commits() {
        let response = execute_with(
            ProjectGitScript {
                present: true,
                branch: "main".into(),
                has_remote: true,
                upstream: Some("origin/main".into()),
                rev_list_count: 2,
                push_result: SyncOutput {
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
        assert_eq!(
            response.ledger.entries(),
            &[PushLedgerEntry {
                repository_name: crate::utils::project_name("a"),
                ahead: CommitCount::new(2),
                checked_at: gtl_models::timestamps::MachineTimestamp::try_from(
                    "2026-07-03T00:00:00Z",
                )
                .expect("fixture ledger timestamp is valid"),
            }]
        );
    }

    #[tokio::test]
    async fn dry_run_reports_would_push_without_treating_it_as_pushed() {
        let remote = ProjectGitScript {
            present: true,
            branch: "main".into(),
            has_remote: true,
            rev_list_count: 1,
            push_result: SyncOutput {
                success: true,
                combined: "would push".into(),
            },
            ..Default::default()
        };
        let mut request = req();
        request.mode = GitEffectMode::DryRun;

        let response = execute_with(remote, vec![repo("a")], request)
            .await
            .expect("push succeeds");

        assert_eq!(response.results[0].status, SyncStatus::WouldPush);
    }

    #[tokio::test]
    async fn a_rejected_push_reports_fail_and_the_exit_precedence_wins() {
        let response = execute_with(
            ProjectGitScript {
                present: true,
                branch: "main".into(),
                has_remote: true,
                rev_list_count: 1,
                push_result: SyncOutput {
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
            ProjectGitScript {
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
            ProjectGitScript {
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
    async fn a_project_client_failure_propagates_as_an_error() {
        let error = push_repositories::execute(
            req(),
            &ProjectGitScript::default().git_client(),
            &FakeProjectClient {
                repos: Vec::new(),
                error: Some("boom".into()),
            },
            &FixedClock::from_raw("2026-07-03T00:00:00Z"),
        )
        .await
        .expect_err("project client error propagates");
        assert!(format!("{error:#}").contains("boom"));
    }
}
