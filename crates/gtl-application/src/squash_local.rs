//! Squashes a repository's unpushed commits while preserving its pre-squash tree.

use std::path::{Path, PathBuf};

use crate::ports::{GitClient, GitEffect};

/// Requests collapsing every unpushed commit into one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashLocal {
    pub repo_path: PathBuf,
    pub message: String,
    pub dry: bool,
}

/// Classifies the closed outcome of a local squash attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquashStatus {
    /// The repository or request cannot be squashed.
    Refused,
    /// The repository already has zero or one unpushed commit.
    Noop,
    /// A dry run found commits that would be squashed.
    WouldSquash,
    /// The unpushed commits were collapsed successfully.
    Squashed,
    /// Git rejected an operation or verification failed.
    Failed,
}

/// Reports the squash status, selected commits, and recovery position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashLocalOk {
    pub status: SquashStatus,
    pub count: usize,
    pub commits: Vec<String>,
    pub pre: String,
    pub detail: String,
}

impl SquashLocalOk {
    fn new(status: SquashStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            count: 0,
            commits: Vec::new(),
            pre: String::new(),
            detail: detail.into(),
        }
    }
}

/// Reports an unexpected Git transport or recovery failure while squashing local commits.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SquashLocalError {
    /// Git could not be started or its output could not be collected.
    #[error("{0}")]
    Unexpected(#[source] anyhow::Error),
    /// Git rejected recovery after the primary transport failure.
    #[error("{source:#}; recovery `git reset --soft {pre}` was rejected: {detail}")]
    RecoveryRejected {
        /// The pre-squash commit recovery attempted to restore.
        pre: String,
        /// Git's diagnostic for the rejected recovery.
        detail: String,
        /// The primary Git transport failure.
        #[source]
        source: anyhow::Error,
    },
    /// Git transport failed while recovery was being attempted.
    #[error("{primary}; recovery `git reset --soft {pre}` failed: {source:#}")]
    RecoveryTransport {
        /// The primary operation failure, including its transport chain when applicable.
        primary: String,
        /// The pre-squash commit recovery attempted to restore.
        pre: String,
        /// The recovery Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClosedFailure {
    CommitRejected { detail: String },
    ByteCheckFailed,
}

impl ClosedFailure {
    fn primary_detail(&self) -> String {
        match self {
            Self::CommitRejected { detail } => format!("commit failed: {detail}"),
            Self::ByteCheckFailed => "BYTE-CHECK FAILED. Tree differed".into(),
        }
    }

    fn restored_detail(&self, pre: &str) -> String {
        match self {
            Self::CommitRejected { detail } => {
                format!("commit failed: {detail} — restored to {pre}")
            }
            Self::ByteCheckFailed => {
                format!("BYTE-CHECK FAILED — restored to {pre}. Tree differed; nothing changed.")
            }
        }
    }
}

#[derive(Debug)]
struct ClosedFailureContext {
    failure: ClosedFailure,
    count: usize,
    commits: Vec<String>,
    pre: String,
}

#[derive(Debug)]
enum RestoreOutcome {
    Restored,
    Rejected { detail: String },
    Transport(anyhow::Error),
}

/// Preflights and optionally collapses the repository's unpushed commits.
///
/// # Errors
///
/// Returns [`SquashLocalError`] when Git cannot be executed, including while restoring the
/// pre-squash position after a destructive step.
#[cqrsy::command]
pub fn execute(
    command: SquashLocal,
    git: &impl GitClient,
) -> Result<SquashLocalOk, SquashLocalError> {
    let SquashLocal {
        repo_path,
        message,
        dry,
    } = command;
    let Some(top) = git
        .discover_top(&repo_path)
        .map_err(SquashLocalError::Unexpected)?
    else {
        return Ok(SquashLocalOk::new(SquashStatus::Refused, "not a git repo"));
    };

    let upstream = match git.upstream(&top) {
        Ok(GitEffect::Applied(upstream)) if !upstream.is_empty() => upstream,
        Ok(GitEffect::Applied(_) | GitEffect::Rejected(_)) => {
            return Ok(SquashLocalOk::new(
                SquashStatus::Refused,
                "no upstream tracking branch (run: git push -u origin <branch>)",
            ));
        }
        Err(source) => return Err(SquashLocalError::Unexpected(source)),
    };

    if message.is_empty() {
        return Ok(SquashLocalOk::new(
            SquashStatus::Refused,
            "commit message is required",
        ));
    }

    let range = format!("{upstream}..HEAD");
    let Some(count) = git
        .commit_count(&top, &range)
        .map_err(SquashLocalError::Unexpected)?
    else {
        return Ok(SquashLocalOk::new(
            SquashStatus::Failed,
            "rev-list returned invalid count",
        ));
    };

    if count == 0 {
        return Ok(SquashLocalOk {
            count,
            ..SquashLocalOk::new(
                SquashStatus::Noop,
                format!("nothing unpushed (HEAD == {upstream})"),
            )
        });
    }

    let commits = match git
        .brief_log(&top, &range)
        .map_err(SquashLocalError::Unexpected)?
    {
        GitEffect::Applied(commits) => commits,
        GitEffect::Rejected(detail) => {
            return Ok(SquashLocalOk {
                count,
                ..SquashLocalOk::new(SquashStatus::Failed, format!("log failed: {detail}"))
            });
        }
    };

    if count == 1 {
        return Ok(SquashLocalOk {
            count,
            commits,
            ..SquashLocalOk::new(
                SquashStatus::Noop,
                "already one commit ahead — nothing to collapse",
            )
        });
    }

    if dry {
        return Ok(SquashLocalOk {
            count,
            commits,
            ..SquashLocalOk::new(
                SquashStatus::WouldSquash,
                format!("would collapse {count} commits into one"),
            )
        });
    }

    collapse(git, &top, &upstream, &message, count, commits)
}

fn collapse(
    git: &impl GitClient,
    top: &Path,
    upstream: &str,
    message: &str,
    count: usize,
    commits: Vec<String>,
) -> Result<SquashLocalOk, SquashLocalError> {
    let pre = git
        .resolve_sha(top, "HEAD")
        .map_err(SquashLocalError::Unexpected)?;

    if let GitEffect::Rejected(detail) = git
        .soft_reset(top, upstream)
        .map_err(SquashLocalError::Unexpected)?
    {
        return Ok(SquashLocalOk {
            status: SquashStatus::Failed,
            count,
            commits,
            pre,
            detail: format!("reset --soft failed: {detail}; no changes made."),
        });
    }

    let commit = match git.commit(top, message) {
        Ok(commit) => commit,
        Err(source) => {
            return Err(restore_after_transport_failure(git, top, &pre, source));
        }
    };
    if let GitEffect::Rejected(detail) = commit {
        return restore_closed_failure(
            git,
            top,
            ClosedFailureContext {
                failure: ClosedFailure::CommitRejected { detail },
                count,
                commits,
                pre,
            },
        );
    }

    let diff = match git.diff_stat(top, &pre, "HEAD") {
        Ok(diff) => diff,
        Err(source) => {
            return Err(restore_after_transport_failure(git, top, &pre, source));
        }
    };
    if matches!(diff, GitEffect::Applied(ref stat) if stat.trim().is_empty()) {
        return Ok(SquashLocalOk {
            status: SquashStatus::Squashed,
            count,
            commits,
            pre: pre.clone(),
            detail: format!(
                "collapsed {count} commits into one. recover with: git reset --soft {pre}"
            ),
        });
    }

    restore_closed_failure(
        git,
        top,
        ClosedFailureContext {
            failure: ClosedFailure::ByteCheckFailed,
            count,
            commits,
            pre,
        },
    )
}

fn restore_closed_failure(
    git: &impl GitClient,
    top: &Path,
    context: ClosedFailureContext,
) -> Result<SquashLocalOk, SquashLocalError> {
    let ClosedFailureContext {
        failure,
        count,
        commits,
        pre,
    } = context;
    let detail = match restore(git, top, &pre) {
        RestoreOutcome::Restored => failure.restored_detail(&pre),
        RestoreOutcome::Rejected { detail: rejected } => {
            format!(
                "{}; recovery `git reset --soft {}` was rejected: {rejected}",
                failure.primary_detail(),
                pre
            )
        }
        RestoreOutcome::Transport(source) => {
            return Err(SquashLocalError::RecoveryTransport {
                primary: failure.primary_detail(),
                pre,
                source,
            });
        }
    };
    Ok(SquashLocalOk {
        status: SquashStatus::Failed,
        count,
        commits,
        pre,
        detail,
    })
}

fn restore_after_transport_failure(
    git: &impl GitClient,
    top: &Path,
    pre: &str,
    source: anyhow::Error,
) -> SquashLocalError {
    match restore(git, top, pre) {
        RestoreOutcome::Restored => SquashLocalError::Unexpected(source),
        RestoreOutcome::Rejected { detail } => SquashLocalError::RecoveryRejected {
            pre: pre.into(),
            detail,
            source,
        },
        RestoreOutcome::Transport(restore_source) => SquashLocalError::RecoveryTransport {
            primary: format!("{source:#}"),
            pre: pre.into(),
            source: restore_source,
        },
    }
}

fn restore(git: &impl GitClient, top: &Path, pre: &str) -> RestoreOutcome {
    match git.soft_reset(top, pre) {
        Ok(GitEffect::Applied(())) => RestoreOutcome::Restored,
        Ok(GitEffect::Rejected(detail)) => RestoreOutcome::Rejected { detail },
        Err(source) => RestoreOutcome::Transport(source),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{SquashLocal, SquashLocalOk, SquashStatus, execute};
    use crate::testing::ScriptedGitClient;

    fn command(dry: bool) -> SquashLocal {
        SquashLocal {
            repo_path: ".".into(),
            message: "collapse".into(),
            dry,
        }
    }

    fn preflight(count: &str) -> Vec<crate::testing::GitResponse> {
        vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied(count),
        ]
    }

    #[test]
    fn one_unpushed_commit_is_a_closed_noop_value() {
        let mut outputs = preflight("1\n");
        outputs.push(ScriptedGitClient::applied("abc1234 one commit\n"));
        let git = ScriptedGitClient::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashLocalOk {
                status: SquashStatus::Noop,
                count: 1,
                commits: vec!["abc1234 one commit".into()],
                pre: String::new(),
                detail: "already one commit ahead — nothing to collapse".into(),
            }
        );
    }

    #[test]
    fn invalid_count_is_a_closed_failed_value() {
        let git = ScriptedGitClient::new(preflight("not-a-count\n"));

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashLocalOk {
                status: SquashStatus::Failed,
                count: 0,
                commits: Vec::new(),
                pre: String::new(),
                detail: "rev-list returned invalid count".into(),
            }
        );
    }

    #[test]
    fn byte_check_failure_is_a_closed_restored_value() {
        let mut outputs = preflight("2\n");
        outputs.extend([
            ScriptedGitClient::applied("abc1234 first\nfed5678 second\n"),
            ScriptedGitClient::applied("pre123\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(" changed.txt | 1 +\n"),
            ScriptedGitClient::applied(""),
        ]);
        let git = ScriptedGitClient::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashLocalOk {
                status: SquashStatus::Failed,
                count: 2,
                commits: vec!["abc1234 first".into(), "fed5678 second".into()],
                pre: "pre123".into(),
                detail: "BYTE-CHECK FAILED — restored to pre123. Tree differed; nothing changed."
                    .into(),
            }
        );
    }

    #[test]
    fn commit_failure_is_a_closed_restored_value() {
        let mut outputs = preflight("2\n");
        outputs.extend([
            ScriptedGitClient::applied("abc1234 first\nfed5678 second\n"),
            ScriptedGitClient::applied("pre123\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("commit rejected"),
            ScriptedGitClient::applied(""),
        ]);
        let git = ScriptedGitClient::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashLocalOk {
                status: SquashStatus::Failed,
                count: 2,
                commits: vec!["abc1234 first".into(), "fed5678 second".into()],
                pre: "pre123".into(),
                detail: "commit failed: commit rejected — restored to pre123".into(),
            }
        );
    }

    #[test]
    fn commit_failure_preserves_primary_and_pre_when_restore_is_rejected() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(ScriptedGitClient::applied(
                "abc1234 first\nfed5678 second\n",
            )),
            Ok(ScriptedGitClient::applied("pre123\n")),
            Ok(ScriptedGitClient::applied("")),
            Ok(ScriptedGitClient::rejected("commit rejected")),
            Ok(ScriptedGitClient::rejected("restore rejected")),
        ]);
        let git = ScriptedGitClient::with_results(results);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashLocalOk {
                status: SquashStatus::Failed,
                count: 2,
                commits: vec!["abc1234 first".into(), "fed5678 second".into()],
                pre: "pre123".into(),
                detail: "commit failed: commit rejected; recovery `git reset --soft pre123` was rejected: restore rejected".into(),
            }
        );
    }

    #[test]
    fn byte_check_failure_preserves_primary_and_pre_when_restore_transport_fails() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(ScriptedGitClient::applied(
                "abc1234 first\nfed5678 second\n",
            )),
            Ok(ScriptedGitClient::applied("pre123\n")),
            Ok(ScriptedGitClient::applied("")),
            Ok(ScriptedGitClient::applied("")),
            Ok(ScriptedGitClient::applied(" changed.txt | 1 +\n")),
            Err(anyhow::anyhow!("restore pipe closed").context("restore transport failed")),
        ]);
        let git = ScriptedGitClient::with_results(results);

        let error =
            execute(command(false), &git).expect_err("restore transport must remain an error");

        assert_eq!(
            error.to_string(),
            "BYTE-CHECK FAILED. Tree differed; recovery `git reset --soft pre123` failed: restore transport failed: restore pipe closed"
        );
        let recovery_source = error.source().expect("recovery source should be retained");
        assert_eq!(recovery_source.to_string(), "restore transport failed");
        assert_eq!(
            recovery_source.source().map(ToString::to_string),
            Some("restore pipe closed".into())
        );
    }

    #[test]
    fn transport_failure_preserves_primary_source_when_restore_is_rejected() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(ScriptedGitClient::applied(
                "abc1234 first\nfed5678 second\n",
            )),
            Ok(ScriptedGitClient::applied("pre123\n")),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("commit pipe closed").context("commit transport failed")),
            Ok(ScriptedGitClient::rejected("restore rejected")),
        ]);
        let git = ScriptedGitClient::with_results(results);

        let error =
            execute(command(false), &git).expect_err("primary transport must remain an error");

        assert_eq!(
            error.to_string(),
            "commit transport failed: commit pipe closed; recovery `git reset --soft pre123` was rejected: restore rejected"
        );
        let primary_source = error.source().expect("primary source should be retained");
        assert_eq!(primary_source.to_string(), "commit transport failed");
        assert_eq!(
            primary_source.source().map(ToString::to_string),
            Some("commit pipe closed".into())
        );
    }

    #[test]
    fn both_transport_failures_preserve_primary_context_and_recovery_source() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(ScriptedGitClient::applied(
                "abc1234 first\nfed5678 second\n",
            )),
            Ok(ScriptedGitClient::applied("pre123\n")),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("commit pipe closed").context("commit transport failed")),
            Err(anyhow::anyhow!("restore pipe closed").context("restore transport failed")),
        ]);
        let git = ScriptedGitClient::with_results(results);

        let error =
            execute(command(false), &git).expect_err("transport failures must remain errors");

        assert_eq!(
            error.to_string(),
            "commit transport failed: commit pipe closed; recovery `git reset --soft pre123` failed: restore transport failed: restore pipe closed"
        );
        let recovery_source = error.source().expect("recovery source should be retained");
        assert_eq!(recovery_source.to_string(), "restore transport failed");
        assert_eq!(
            recovery_source.source().map(ToString::to_string),
            Some("restore pipe closed".into())
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(command(false), &git).expect_err("transport failure must be an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
