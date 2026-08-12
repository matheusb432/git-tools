//! Commits dirty working trees across already-resolved managed repositories.

use gtl_models::managed::{ManagedRepo, working_tree::CommitFile};

use super::working_tree;
use crate::ports::{GitClient, GitEffect};

/// Requests one local commit attempt for every resolved managed repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitAll {
    pub repos: Vec<ManagedRepo>,
    pub message: Option<String>,
    pub dry: bool,
}

/// Classifies what the managed commit operation did with one repository.
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

/// Reports one repository's commit decision and resulting local commit, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    pub name: String,
    pub present: bool,
    pub dirty: bool,
    pub files: Vec<CommitFile>,
    pub action: CommitAction,
    pub detail: String,
    pub commit: Option<String>,
    /// True when `git add -A` completed successfully for this attempt.
    pub staged: bool,
}

/// Classifies the aggregate managed commit result for CLI exit mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitExit {
    Clean,
    Warn,
    Fail,
}

/// Reports every repository result and the aggregate exit classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitAllOk {
    pub exit: CommitExit,
    pub results: Vec<CommitResult>,
}

/// Reports an unexpected Git transport failure while committing managed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommitAllError {
    /// Git transport failed while processing one repository.
    #[error("managed commit failed for '{failed_repo}': {source}")]
    Transport {
        /// Identifies the repository whose Git invocation failed.
        failed_repo: String,
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
/// Returns [`CommitAllError`] when Git cannot be executed while staging or committing.
#[cqrsy::command]
pub fn execute(command: CommitAll, git: &impl GitClient) -> Result<CommitAllOk, CommitAllError> {
    let CommitAll {
        repos,
        message,
        dry,
    } = command;
    let mut results = Vec::with_capacity(repos.len());
    for repo in &repos {
        match commit_one(git, repo, message.as_deref(), dry) {
            Ok(result) => results.push(result),
            Err(error) => {
                return Err(CommitAllError::Transport {
                    failed_repo: repo.name.clone(),
                    completed_results: results,
                    failed_result: error.failed_result.map(Box::new),
                    source: error.source,
                });
            }
        }
    }

    Ok(CommitAllOk {
        exit: classify_exit(&results, dry),
        results,
    })
}

fn commit_one(
    git: &impl GitClient,
    repo: &ManagedRepo,
    message: Option<&str>,
    dry: bool,
) -> Result<CommitResult, CommitAttemptError> {
    let state = working_tree::read(git, &repo.path).map_err(|source| CommitAttemptError {
        failed_result: None,
        source,
    })?;
    let mut result = CommitResult {
        name: repo.name.clone(),
        present: state.present,
        dirty: state.dirty,
        files: state.files,
        action: CommitAction::Clean,
        detail: String::new(),
        commit: None,
        staged: false,
    };

    if !state.present {
        result.action = CommitAction::Absent;
        result.detail = "not present on this machine".into();
        return Ok(result);
    }
    if !state.dirty {
        result.detail = "nothing to commit".into();
        return Ok(result);
    }
    if dry {
        result.action = CommitAction::WouldCommit;
        result.detail = format!("{} change(s)", result.files.len());
        return Ok(result);
    }
    let Some(message) = message else {
        result.action = CommitAction::Skipped;
        result.detail = "blank message - skipped".into();
        return Ok(result);
    };

    let output = match git.stage_all(&repo.path) {
        Ok(output) => output,
        Err(source) => {
            result.action = CommitAction::Fail;
            result.detail = "git add failed".into();
            return Err(CommitAttemptError {
                failed_result: Some(result),
                source,
            });
        }
    };
    if let GitEffect::Rejected(_) = output {
        result.action = CommitAction::Fail;
        result.detail = "git add failed".into();
        return Ok(result);
    }
    result.staged = true;

    let output = match git.commit(&repo.path, message) {
        Ok(output) => output,
        Err(source) => {
            result.action = CommitAction::Fail;
            result.detail = "git commit failed".into();
            return Err(CommitAttemptError {
                failed_result: Some(result),
                source,
            });
        }
    };
    match output {
        GitEffect::Applied(receipt) => {
            result.action = CommitAction::Committed;
            result.detail = receipt.detail;
            result.commit = receipt.identity;
        }
        GitEffect::Rejected(detail) => {
            result.action = CommitAction::Fail;
            result.detail = detail;
        }
    }
    Ok(result)
}

fn classify_exit(results: &[CommitResult], dry: bool) -> CommitExit {
    if results
        .iter()
        .any(|result| result.action == CommitAction::Fail)
    {
        return CommitExit::Fail;
    }
    if dry
        && results
            .iter()
            .any(|result| result.action == CommitAction::WouldCommit)
    {
        return CommitExit::Warn;
    }
    if !dry
        && results
            .iter()
            .any(|result| result.action == CommitAction::Skipped)
    {
        return CommitExit::Warn;
    }
    CommitExit::Clean
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use gtl_models::managed::{ManagedRepo, working_tree::CommitFile};

    use super::{CommitAction, CommitAll, CommitAllError, CommitExit, CommitResult, execute};
    use crate::testing::ScriptedGitClient;

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: String::new(),
        }
    }

    fn commit(repos: Vec<ManagedRepo>, message: Option<&str>, dry: bool) -> CommitAll {
        CommitAll {
            repos,
            message: message.map(str::to_string),
            dry,
        }
    }

    #[test]
    fn successful_managed_commit_reports_the_created_commit() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[main abc1234] save\n"),
        ]);

        let result = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect("Git transport remains available");

        assert_eq!(result.exit, CommitExit::Clean);
        assert_eq!(
            result.results,
            vec![CommitResult {
                name: "api".into(),
                present: true,
                dirty: true,
                files: vec![CommitFile {
                    status: "M".into(),
                    path: "src/lib.rs".into(),
                }],
                action: CommitAction::Committed,
                detail: "[main abc1234] save".into(),
                commit: Some("abc1234".into()),
                staged: true,
            }]
        );
    }

    #[test]
    fn commit_metadata_skips_bracketed_hook_noise() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[lint passed]\n[main abc1234] save\n"),
        ]);

        let result = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect("Git transport remains available");

        assert_eq!(result.results[0].commit.as_deref(), Some("abc1234"));
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

        let result = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect("Git transport remains available");

        assert_eq!(result.results[0].commit.as_deref(), Some("abc1234"));
    }

    #[test]
    fn absent_and_clean_repositories_are_closed_clean_results() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);
        git.absent_repos.lock().unwrap().push("/repos/api".into());

        let result = execute(
            commit(vec![repo("api"), repo("web")], Some("save"), false),
            &git,
        )
        .expect("Git transport remains available");

        assert_eq!(result.exit, CommitExit::Clean);
        assert_eq!(result.results[0].action, CommitAction::Absent);
        assert_eq!(result.results[0].detail, "not present on this machine");
        assert_eq!(result.results[1].action, CommitAction::Clean);
        assert_eq!(result.results[1].detail, "nothing to commit");
    }

    #[test]
    fn dry_run_reports_dirty_files_and_warns_without_a_message() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("?? notes.txt\n")]);

        let result = execute(commit(vec![repo("api")], None, true), &git)
            .expect("Git transport remains available");

        assert_eq!(result.exit, CommitExit::Warn);
        assert_eq!(result.results[0].action, CommitAction::WouldCommit);
        assert_eq!(result.results[0].detail, "1 change(s)");
        assert_eq!(result.results[0].commit, None);
    }

    #[test]
    fn missing_real_message_is_a_closed_skip_and_warn() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(" M src/lib.rs\n")]);

        let result = execute(commit(vec![repo("api")], None, false), &git)
            .expect("Git transport remains available");

        assert_eq!(result.exit, CommitExit::Warn);
        assert_eq!(result.results[0].action, CommitAction::Skipped);
        assert_eq!(result.results[0].detail, "blank message - skipped");
    }

    #[test]
    fn git_add_rejection_is_a_closed_failure_and_fail_exit() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::rejected("fatal: index locked"),
        ]);

        let result = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect("a Git rejection is a closed commit failure");

        assert_eq!(result.exit, CommitExit::Fail);
        assert_eq!(result.results[0].action, CommitAction::Fail);
        assert_eq!(result.results[0].detail, "git add failed");
    }

    #[test]
    fn git_commit_rejection_preserves_the_last_detail_line() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("hint: resolve it\nfatal: commit rejected\n"),
        ]);

        let result = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect("a Git rejection is a closed commit failure");

        assert_eq!(result.exit, CommitExit::Fail);
        assert_eq!(result.results[0].action, CommitAction::Fail);
        assert_eq!(result.results[0].detail, "fatal: commit rejected");
        assert!(result.results[0].staged);
    }

    #[test]
    fn working_tree_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "managed commit failed for 'api': git transport unavailable"
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

        let error = execute(
            commit(vec![repo("api"), repo("web")], Some("save"), false),
            &git,
        )
        .expect_err("the second repository transport must fail");

        let CommitAllError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo, "web");
        assert_eq!(completed_results.len(), 1);
        assert_eq!(completed_results[0].name, "api");
        assert_eq!(completed_results[0].action, CommitAction::Committed);
        assert_eq!(completed_results[0].commit.as_deref(), Some("abc1234"));
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

        let error = execute(commit(vec![repo("api")], Some("save"), false), &git)
            .expect_err("the commit transport must fail");

        let CommitAllError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo, "api");
        assert_eq!(completed_results, Vec::new());
        let failed_result = failed_result.expect("known dirty state must be preserved");
        assert_eq!(failed_result.name, "api");
        assert_eq!(failed_result.action, CommitAction::Fail);
        assert_eq!(failed_result.detail, "git commit failed");
        assert_eq!(failed_result.files.len(), 1);
        assert!(failed_result.staged);
        assert_eq!(source.to_string(), "commit transport unavailable");
    }
}
