//! The `push_all` vertical slice: select eligible active projects, then fan out
//! `git push` while preserving the retired `push_pull.rs::push_one` status and detail semantics.

use futures_util::{StreamExt as _, stream};
use gtl_models::{
    failure::ErrorMeta,
    git::{CommitCount, GitEffectMode, GitRange, RemoteName},
    paths::ProjectName,
    projects::{ProjectRepository, push_ledger::PushLedger},
};

use crate::{
    ports::{
        Clock, GitClient, GitEffect, ProjectClient, ProjectClientError, UserSettingsLoadError,
        UserSettingsReader,
    },
    projects::remote_sync::{self, Preflight, RepoSyncResult, SyncExit, SyncStatus},
};

const MAX_CONCURRENT_GIT_OPERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PushRepositoriesOk {
    pub selected: Vec<RepoSyncResult>,
    pub excluded: Vec<ProjectName>,
    pub exit: SyncExit,
    pub ledger: PushLedger,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PushRepositoriesError {
    #[error(transparent)]
    #[meta(transparent)]
    ProjectClient(#[from] ProjectClientError),
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Pushes every selected project repository through the Git capability.
#[cqrsy::command]
pub async fn execute(
    mode: GitEffectMode,
    git: &impl GitClient,
    projects: &impl ProjectClient,
    clock: &impl Clock,
    user_settings: &impl UserSettingsReader,
) -> Result<PushRepositoriesOk, PushRepositoriesError> {
    let settings = user_settings.load()?;
    let selection = super::select_push_all_repositories(
        projects.list_projects().await?,
        settings.push_all_exclusions(),
    );
    let tasks = stream::iter(selection.selected.into_iter().enumerate())
        .map(|(index, repo)| push_repository(index, repo, mode, git.clone()))
        .buffer_unordered(MAX_CONCURRENT_GIT_OPERATIONS)
        .collect::<Vec<_>>()
        .await;
    let mut completed = tasks.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
    completed.sort_by_key(|(index, _, _)| *index);
    let mut selected = Vec::with_capacity(completed.len());
    let mut ledger = PushLedger::default();
    for (_, repo_name, outcome) in completed {
        if let Some(ahead) = outcome.ledger_ahead {
            ledger.record(repo_name, ahead, clock.now().map_err(anyhow::Error::from)?);
        }
        selected.push(outcome.result);
    }
    let exit = remote_sync::classify_exit(&selected);
    Ok(PushRepositoriesOk {
        selected,
        excluded: selection.excluded,
        exit,
        ledger,
    })
}

async fn push_repository(
    index: usize,
    repo: ProjectRepository,
    mode: GitEffectMode,
    git: impl GitClient,
) -> anyhow::Result<(usize, ProjectName, PushOneOutcome)> {
    tokio::task::spawn_blocking(move || {
        let outcome = push_one(&git, &repo, mode);
        (index, repo.name, outcome)
    })
    .await
    .map_err(anyhow::Error::from)
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
    use gtl_models::{
        diffs::DiffExclusions,
        projects::{ProjectRepository, push_ledger::PushLedgerEntry},
        settings::{PushAllExclusions, UserSettings},
        viewer::RenderOptions,
    };

    use super::*;
    use crate::{
        ports::ProjectCatalogueUnavailableError,
        projects::push_repositories,
        utils::{
            FakeProjectClient, FixedClock, FixedUserSettingsStore, ProjectGitScript,
            SequenceUserSettingsStore, SyncOutput,
        },
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
    ) -> Result<PushRepositoriesOk, PushRepositoriesError> {
        execute_with_settings(remote, repos, mode, FixedUserSettingsStore::default()).await
    }

    async fn execute_with_settings(
        remote: ProjectGitScript,
        repos: Vec<ProjectRepository>,
        mode: GitEffectMode,
        settings: FixedUserSettingsStore,
    ) -> Result<PushRepositoriesOk, PushRepositoriesError> {
        let projects = FakeProjectClient { repos, error: None };
        let git = remote.git_client();
        push_repositories::execute(
            mode,
            &git,
            &projects,
            &FixedClock::from_raw("2026-07-03T00:00:00Z"),
            &settings,
        )
        .await
    }

    fn settings_excluding(projects: &[&str]) -> FixedUserSettingsStore {
        FixedUserSettingsStore::new(UserSettings::new(
            None,
            RenderOptions::DEFAULT,
            gtl_models::viewer::ViewerKeybindings::default(),
            true,
            DiffExclusions::default(),
            PushAllExclusions::new(
                projects
                    .iter()
                    .map(|project| crate::utils::project_name(project)),
            ),
        ))
    }

    fn req() -> GitEffectMode {
        GitEffectMode::Apply
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
        .unwrap();

        assert_eq!(response.selected.len(), 1);
        assert_eq!(response.selected[0].status, SyncStatus::UpToDate);
        assert_eq!(response.selected[0].detail, "up to date (already synced)");
        assert_eq!(response.exit, SyncExit::Clean);
        assert_eq!(
            response.ledger.entries(),
            &[PushLedgerEntry {
                repository_name: crate::utils::project_name("a"),
                ahead: CommitCount::default(),
                checked_at: gtl_models::timestamps::MachineTimestamp::try_from(
                    "2026-07-03T00:00:00Z",
                )
                .unwrap(),
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
        .unwrap();

        assert_eq!(response.selected[0].status, SyncStatus::Pushed);
        assert_eq!(response.selected[0].detail, "abc..def  main -> main");
        assert_eq!(response.exit, SyncExit::Clean);
        assert_eq!(
            response.ledger.entries(),
            &[PushLedgerEntry {
                repository_name: crate::utils::project_name("a"),
                ahead: CommitCount::new(2),
                checked_at: gtl_models::timestamps::MachineTimestamp::try_from(
                    "2026-07-03T00:00:00Z",
                )
                .unwrap(),
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
        let request = GitEffectMode::DryRun;

        let response = execute_with(remote, vec![repo("a")], request)
            .await
            .unwrap();

        assert_eq!(response.selected[0].status, SyncStatus::WouldPush);
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
        .unwrap();

        assert_eq!(response.selected[0].status, SyncStatus::Fail);
        assert_eq!(
            response.selected[0].detail,
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
        .unwrap();

        assert_eq!(response.selected[0].status, SyncStatus::Warn);
        assert_eq!(
            response.selected[0].detail,
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
        .unwrap();

        assert_eq!(response.selected.len(), 3);
        assert!(response.excluded.is_empty());
        assert_eq!(
            response
                .selected
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[tokio::test]
    async fn exclusions_partition_repositories_before_dry_run_git_operations() {
        let request = GitEffectMode::DryRun;
        let response = execute_with_settings(
            ProjectGitScript {
                present: true,
                branch: "main".into(),
                has_remote: true,
                upstream: Some("origin/main".into()),
                rev_list_count: 1,
                push_result: SyncOutput {
                    success: true,
                    combined: "would push".into(),
                },
                ..Default::default()
            },
            vec![repo("excluded"), repo("selected")],
            request,
            settings_excluding(&["excluded"]),
        )
        .await
        .unwrap();

        assert_eq!(
            response
                .selected
                .iter()
                .map(|result| result.name.as_str())
                .collect::<Vec<_>>(),
            ["selected"]
        );
        assert_eq!(response.selected[0].status, SyncStatus::WouldPush);
        assert_eq!(
            response
                .excluded
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["excluded"]
        );
    }

    #[tokio::test]
    async fn unknown_and_differently_cased_exclusion_names_are_inert() {
        let response = execute_with_settings(
            ProjectGitScript {
                present: false,
                ..Default::default()
            },
            vec![repo("selected")],
            req(),
            settings_excluding(&["Selected", "not-active"]),
        )
        .await
        .unwrap();

        assert_eq!(response.selected.len(), 1);
        assert!(response.excluded.is_empty());
    }

    #[tokio::test]
    async fn all_excluded_repositories_return_a_clean_result_without_selected_work() {
        let response = execute_with_settings(
            ProjectGitScript::default(),
            vec![repo("a"), repo("b")],
            req(),
            settings_excluding(&["a", "b"]),
        )
        .await
        .unwrap();

        assert!(response.selected.is_empty());
        assert_eq!(
            response
                .excluded
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(response.exit, SyncExit::Clean);
        assert!(response.ledger.entries().is_empty());
    }

    #[tokio::test]
    async fn settings_adapter_failure_preserves_its_typed_category() {
        let error = push_repositories::execute(
            req(),
            &ProjectGitScript::default().git_client(),
            &FakeProjectClient::default(),
            &FixedClock::from_raw("2026-07-03T00:00:00Z"),
            &SequenceUserSettingsStore::new([]),
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            PushRepositoriesError::Settings(UserSettingsLoadError::Adapter(_))
        ));
    }

    #[tokio::test]
    async fn a_project_client_failure_propagates_as_an_error() -> anyhow::Result<()> {
        let error = push_repositories::execute(
            req(),
            &ProjectGitScript::default().git_client(),
            &FakeProjectClient {
                repos: Vec::new(),
                error: Some("boom".into()),
            },
            &FixedClock::from_raw("2026-07-03T00:00:00Z"),
            &FixedUserSettingsStore::default(),
        )
        .await
        .unwrap_err();
        let source = match error {
            PushRepositoriesError::ProjectClient(ProjectClientError::Unavailable(
                ProjectCatalogueUnavailableError::Dependency(source),
            )) => Some(source),
            _ => None,
        }
        .unwrap();
        assert_eq!(source.root_cause().to_string(), "boom");
        Ok(())
    }
}
