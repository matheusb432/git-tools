//! Commits dirty working trees across already-resolved project repositories.

use std::borrow::Cow;

use gtl_models::{
    diffs::CommitId,
    paths::ProjectName,
    projects::ProjectRepository,
    repository::working_tree::{ChangedFiles, CommitFile, DirtyState},
    settings::PushAllExclusions,
};

use crate::{
    ports::{GitClient, GitEffect},
    repositories::working_tree,
};

/// Requests one local commit attempt for every resolved project repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRepositories {
    pub repos: Vec<ProjectRepository>,
    pub mode: CommitRepositoriesMode,
    pub scope: CommitRepositoriesScope,
}

/// Selects standalone commit fan-out or the repository set eligible for `push --all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitRepositoriesScope {
    All,
    PushAll { exclusions: PushAllExclusions },
}

/// Selects previewing or applying project commits without admitting a dry run plus message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitRepositoriesMode {
    DryRun,
    Apply { message: Option<String> },
}

/// Classifies what the project commit operation did with one repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitAction {
    /// The repository is not present on this machine.
    Absent,
    /// The repository has no working-tree changes.
    Clean,
    /// A dry run found changes that a real run would commit.
    WouldCommit,
    /// A dirty repository was skipped because no message was supplied.
    Skipped,
    /// The repository's changes were committed locally.
    Committed,
    /// Git rejected staging or committing the repository.
    Fail,
}

/// The furthest local mutation completed before a project commit failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitFailureProgress {
    StatusUnavailable,
    Unstaged(ChangedFiles),
    Staged(ChangedFiles),
}

/// One repository's mutually exclusive project-commit outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Absent,
    Clean,
    WouldCommit {
        files: ChangedFiles,
    },
    Skipped {
        files: ChangedFiles,
    },
    Committed {
        files: ChangedFiles,
        detail: String,
        id: CommitId,
    },
    Failed {
        progress: CommitFailureProgress,
        detail: String,
    },
}

/// Reports one repository's closed commit outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    name: ProjectName,
    outcome: CommitOutcome,
}

impl CommitResult {
    #[must_use]
    pub fn new(name: ProjectName, outcome: CommitOutcome) -> Self {
        Self { name, outcome }
    }

    #[must_use]
    pub fn name(&self) -> &ProjectName {
        &self.name
    }

    #[must_use]
    pub const fn outcome(&self) -> &CommitOutcome {
        &self.outcome
    }

    #[must_use]
    pub const fn action(&self) -> CommitAction {
        match self.outcome {
            CommitOutcome::Absent => CommitAction::Absent,
            CommitOutcome::Clean => CommitAction::Clean,
            CommitOutcome::WouldCommit { .. } => CommitAction::WouldCommit,
            CommitOutcome::Skipped { .. } => CommitAction::Skipped,
            CommitOutcome::Committed { .. } => CommitAction::Committed,
            CommitOutcome::Failed { .. } => CommitAction::Fail,
        }
    }

    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self.outcome, CommitOutcome::Absent)
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        !matches!(
            self.outcome,
            CommitOutcome::Absent
                | CommitOutcome::Clean
                | CommitOutcome::Failed {
                    progress: CommitFailureProgress::StatusUnavailable,
                    ..
                }
        )
    }

    #[must_use]
    pub fn files(&self) -> &[CommitFile] {
        match &self.outcome {
            CommitOutcome::WouldCommit { files }
            | CommitOutcome::Skipped { files }
            | CommitOutcome::Committed { files, .. }
            | CommitOutcome::Failed {
                progress:
                    CommitFailureProgress::Unstaged(files) | CommitFailureProgress::Staged(files),
                ..
            } => files.as_slice(),
            CommitOutcome::Absent
            | CommitOutcome::Clean
            | CommitOutcome::Failed {
                progress: CommitFailureProgress::StatusUnavailable,
                ..
            } => &[],
        }
    }

    #[must_use]
    pub fn detail(&self) -> Cow<'_, str> {
        match &self.outcome {
            CommitOutcome::Absent => Cow::Borrowed("not present on this machine"),
            CommitOutcome::Clean => Cow::Borrowed("nothing to commit"),
            CommitOutcome::WouldCommit { files } => {
                Cow::Owned(format!("{} change(s)", files.len()))
            }
            CommitOutcome::Skipped { .. } => Cow::Borrowed("blank message - skipped"),
            CommitOutcome::Committed { detail, .. } | CommitOutcome::Failed { detail, .. } => {
                Cow::Borrowed(detail)
            }
        }
    }

    #[must_use]
    pub const fn id(&self) -> Option<&CommitId> {
        match &self.outcome {
            CommitOutcome::Committed { id, .. } => Some(id),
            _ => None,
        }
    }

    #[must_use]
    pub const fn staged(&self) -> bool {
        matches!(
            self.outcome,
            CommitOutcome::Committed { .. }
                | CommitOutcome::Failed {
                    progress: CommitFailureProgress::Staged(_),
                    ..
                }
        )
    }
}

/// Classifies the aggregate project commit result for CLI exit mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitExit {
    Clean,
    Warn,
    Fail,
}

/// Reports every repository result and the aggregate exit classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRepositoriesOk {
    pub exit: CommitExit,
    pub results: Vec<CommitResult>,
}

/// Reports an unexpected Git transport failure while committing project repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommitRepositoriesError {
    /// Git transport failed while processing one repository.
    #[error("project commit failed for '{failed_repo}': {source}")]
    Transport {
        /// Identifies the repository whose Git invocation failed.
        failed_repo: ProjectName,
        /// Preserves every repository result completed before the failure.
        completed_results: Vec<CommitResult>,
        /// Preserves the failed repository's known state when discovery completed.
        failed_result: Option<Box<CommitResult>>,
        /// Preserves the original Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

#[derive(Debug)]
struct CommitAttemptError {
    failed_result: Option<CommitResult>,
    source: anyhow::Error,
}

/// Commits each dirty repository locally and aggregates the closed outcomes.
///
/// # Errors
///
/// Returns [`CommitRepositoriesError`] when Git cannot be executed while staging or committing.
#[cqrsy::command]
pub fn execute(
    command: CommitRepositories,
    git: &impl GitClient,
) -> Result<CommitRepositoriesOk, CommitRepositoriesError> {
    let CommitRepositories { repos, mode, scope } = command;
    let repos = match scope {
        CommitRepositoriesScope::All => repos,
        CommitRepositoriesScope::PushAll { exclusions } => {
            super::select_push_all_repositories(repos, &exclusions).selected
        }
    };
    let mut results = Vec::with_capacity(repos.len());
    for repo in &repos {
        match commit_one(git, repo, &mode) {
            Ok(result) => results.push(result),
            Err(error) => {
                return Err(CommitRepositoriesError::Transport {
                    failed_repo: repo.name.clone(),
                    completed_results: results,
                    failed_result: error.failed_result.map(Box::new),
                    source: error.source,
                });
            }
        }
    }

    Ok(CommitRepositoriesOk {
        exit: classify_exit(&results),
        results,
    })
}

fn commit_one(
    git: &impl GitClient,
    repo: &ProjectRepository,
    mode: &CommitRepositoriesMode,
) -> Result<CommitResult, CommitAttemptError> {
    let state = working_tree::read(git, &repo.path).map_err(|source| CommitAttemptError {
        failed_result: None,
        source,
    })?;
    let files = match state {
        DirtyState::Absent => {
            return Ok(CommitResult::new(repo.name.clone(), CommitOutcome::Absent));
        }
        DirtyState::Clean => {
            return Ok(CommitResult::new(repo.name.clone(), CommitOutcome::Clean));
        }
        DirtyState::Unavailable { detail } => {
            return Ok(CommitResult::new(
                repo.name.clone(),
                CommitOutcome::Failed {
                    progress: CommitFailureProgress::StatusUnavailable,
                    detail: if detail.is_empty() {
                        "git status failed".into()
                    } else {
                        format!("git status failed: {detail}")
                    },
                },
            ));
        }
        DirtyState::Dirty(files) => files,
    };

    let message = match mode {
        CommitRepositoriesMode::DryRun => {
            return Ok(CommitResult::new(
                repo.name.clone(),
                CommitOutcome::WouldCommit { files },
            ));
        }
        CommitRepositoriesMode::Apply { message: None } => {
            return Ok(CommitResult::new(
                repo.name.clone(),
                CommitOutcome::Skipped { files },
            ));
        }
        CommitRepositoriesMode::Apply {
            message: Some(message),
        } => message,
    };

    let output = match git.stage_all(&repo.path) {
        Ok(output) => output,
        Err(source) => {
            return Err(CommitAttemptError {
                failed_result: Some(CommitResult::new(
                    repo.name.clone(),
                    CommitOutcome::Failed {
                        progress: CommitFailureProgress::Unstaged(files),
                        detail: "git add failed".into(),
                    },
                )),
                source,
            });
        }
    };
    if let GitEffect::Rejected(_) = output {
        return Ok(CommitResult::new(
            repo.name.clone(),
            CommitOutcome::Failed {
                progress: CommitFailureProgress::Unstaged(files),
                detail: "git add failed".into(),
            },
        ));
    }

    let output = match git.commit(&repo.path, message) {
        Ok(output) => output,
        Err(source) => {
            return Err(CommitAttemptError {
                failed_result: Some(CommitResult::new(
                    repo.name.clone(),
                    CommitOutcome::Failed {
                        progress: CommitFailureProgress::Staged(files),
                        detail: "git commit failed".into(),
                    },
                )),
                source,
            });
        }
    };
    Ok(CommitResult::new(
        repo.name.clone(),
        match output {
            GitEffect::Applied(receipt) => CommitOutcome::Committed {
                files,
                detail: receipt.detail,
                id: receipt.id,
            },
            GitEffect::Rejected(detail) => CommitOutcome::Failed {
                progress: CommitFailureProgress::Staged(files),
                detail,
            },
        },
    ))
}

fn classify_exit(results: &[CommitResult]) -> CommitExit {
    if results
        .iter()
        .any(|result| result.action() == CommitAction::Fail)
    {
        return CommitExit::Fail;
    }
    if results.iter().any(|result| {
        matches!(
            result.action(),
            CommitAction::WouldCommit | CommitAction::Skipped
        )
    }) {
        return CommitExit::Warn;
    }
    CommitExit::Clean
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use gtl_models::{
        projects::ProjectRepository,
        repository::working_tree::{ChangedFiles, CommitFile},
        settings::PushAllExclusions,
    };

    use super::{
        CommitAction, CommitExit, CommitFailureProgress, CommitOutcome, CommitRepositories,
        CommitRepositoriesError, CommitRepositoriesMode, CommitRepositoriesScope, CommitResult,
    };
    use crate::{projects::commit_repositories, utils::ScriptedGitClient};

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!("/repos/{name}")),
            remote: None,
        }
    }

    fn commit(
        repos: Vec<ProjectRepository>,
        message: Option<&str>,
        dry: bool,
    ) -> CommitRepositories {
        CommitRepositories {
            repos,
            mode: if dry {
                CommitRepositoriesMode::DryRun
            } else {
                CommitRepositoriesMode::Apply {
                    message: message.map(str::to_string),
                }
            },
            scope: CommitRepositoriesScope::All,
        }
    }

    #[test]
    fn push_all_scope_excludes_repositories_before_git_inspection() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);
        let result = commit_repositories::execute(
            CommitRepositories {
                repos: vec![repo("excluded"), repo("selected")],
                mode: CommitRepositoriesMode::Apply {
                    message: Some("save".into()),
                },
                scope: CommitRepositoriesScope::PushAll {
                    exclusions: PushAllExclusions::new([crate::utils::project_name("excluded")]),
                },
            },
            &git,
        )
        .unwrap();

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].name().as_str(), "selected");
        assert_eq!(result.results[0].action(), CommitAction::Clean);
    }

    fn changed_files(status: &str, path: &str) -> ChangedFiles {
        ChangedFiles::try_new(vec![CommitFile {
            status: status.into(),
            path: crate::utils::repository_relative_path(path),
        }])
        .unwrap()
    }

    #[test]
    fn successful_project_commit_reports_the_created_commit() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[main abc1234] save\n"),
        ]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap();

        assert_eq!(result.exit, CommitExit::Clean);
        assert_eq!(
            result.results,
            vec![CommitResult::new(
                crate::utils::project_name("api"),
                CommitOutcome::Committed {
                    files: changed_files("M", "src/lib.rs"),
                    detail: "[main abc1234] save".into(),
                    id: crate::utils::commit_id_fixture("abc1234"),
                }
            )]
        );
    }

    #[test]
    fn commit_metadata_skips_bracketed_hook_noise() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[lint passed]\n[main abc1234] save\n"),
        ]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap();

        assert_eq!(
            result.results[0].id(),
            Some(&crate::utils::commit_id_fixture("abc1234"))
        );
    }

    #[test]
    fn commit_metadata_accepts_ansi_colored_git_summary() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(
                "\u{1b}[1m[\u{1b}[m\u{1b}[36mmain\u{1b}[m \u{1b}[33mabc1234\u{1b}[m\u{1b}[1m]\u{1b}[m save\n",
            ),
        ]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap();

        assert_eq!(
            result.results[0].id(),
            Some(&crate::utils::commit_id_fixture("abc1234"))
        );
    }

    #[test]
    fn absent_and_clean_repositories_are_closed_clean_results() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);
        git.absent_repos.lock().unwrap().push("/repos/api".into());

        let result = commit_repositories::execute(
            commit(vec![repo("api"), repo("web")], Some("save"), false),
            &git,
        )
        .unwrap();

        assert_eq!(result.exit, CommitExit::Clean);
        assert_eq!(result.results[0].action(), CommitAction::Absent);
        assert_eq!(result.results[0].detail(), "not present on this machine");
        assert_eq!(result.results[1].action(), CommitAction::Clean);
        assert_eq!(result.results[1].detail(), "nothing to commit");
    }

    #[test]
    fn dry_run_reports_dirty_files_and_warns_without_a_message() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("?? notes.txt\n")]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], None, true), &git).unwrap();

        assert_eq!(result.exit, CommitExit::Warn);
        assert_eq!(result.results[0].action(), CommitAction::WouldCommit);
        assert_eq!(result.results[0].detail(), "1 change(s)");
        assert_eq!(result.results[0].id(), None);
    }

    #[test]
    fn missing_real_message_is_a_closed_skip_and_warn() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(" M src/lib.rs\n")]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], None, false), &git).unwrap();

        assert_eq!(result.exit, CommitExit::Warn);
        assert_eq!(result.results[0].action(), CommitAction::Skipped);
        assert_eq!(result.results[0].detail(), "blank message - skipped");
    }

    #[test]
    fn git_add_rejection_is_a_closed_failure_and_fail_exit() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::rejected("fatal: index locked"),
        ]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap();

        assert_eq!(result.exit, CommitExit::Fail);
        assert_eq!(result.results[0].action(), CommitAction::Fail);
        assert_eq!(result.results[0].detail(), "git add failed");
        assert!(matches!(
            result.results[0].outcome(),
            CommitOutcome::Failed {
                progress: CommitFailureProgress::Unstaged(_),
                ..
            }
        ));
    }

    #[test]
    fn git_commit_rejection_preserves_the_last_detail_line() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("hint: resolve it\nfatal: commit rejected\n"),
        ]);

        let result =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap();

        assert_eq!(result.exit, CommitExit::Fail);
        assert_eq!(result.results[0].action(), CommitAction::Fail);
        assert_eq!(result.results[0].detail(), "fatal: commit rejected");
        assert!(result.results[0].staged());
    }

    #[test]
    fn working_tree_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap_err();

        assert_eq!(
            error.to_string(),
            "project commit failed for 'api': git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn later_transport_failure_preserves_completed_results_and_failed_repo() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied(" M src/lib.rs\n")),
            Ok(ScriptedGitClient::applied("")),
            Ok(ScriptedGitClient::applied("[main abc1234] save\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = commit_repositories::execute(
            commit(vec![repo("api"), repo("web")], Some("save"), false),
            &git,
        )
        .unwrap_err();

        let CommitRepositoriesError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo.as_str(), "web");
        assert_eq!(completed_results.len(), 1);
        assert_eq!(completed_results[0].name().as_str(), "api");
        assert_eq!(completed_results[0].action(), CommitAction::Committed);
        assert_eq!(
            completed_results[0].id(),
            Some(&crate::utils::commit_id_fixture("abc1234"))
        );
        assert_eq!(failed_result, None);
        assert_eq!(source.to_string(), "git transport unavailable");
    }

    #[test]
    fn commit_transport_failure_preserves_the_staged_repo_state() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied(" M src/lib.rs\n")),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("commit transport unavailable")),
        ]);

        let error =
            commit_repositories::execute(commit(vec![repo("api")], Some("save"), false), &git)
                .unwrap_err();

        let CommitRepositoriesError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo.as_str(), "api");
        assert_eq!(completed_results, Vec::new());
        let failed_result = failed_result.unwrap();
        assert_eq!(failed_result.name().as_str(), "api");
        assert_eq!(failed_result.action(), CommitAction::Fail);
        assert_eq!(failed_result.detail(), "git commit failed");
        assert_eq!(failed_result.files().len(), 1);
        assert!(failed_result.staged());
        assert_eq!(source.to_string(), "commit transport unavailable");
    }
}
