//! Squashes a repository's unpushed commits while preserving its pre-squash tree.

use std::path::{Path, PathBuf};

use crate::ports::GitRunner;

/// Requests collapsing every unpushed commit into one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashLocal {
    pub repo: PathBuf,
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
pub struct SquashResult {
    pub status: SquashStatus,
    pub count: usize,
    pub commits: Vec<String>,
    pub pre: String,
    pub detail: String,
}

impl SquashResult {
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
    #[error("{source:#}; recovery `git reset --soft {pre}` failed (exit {exit_code})")]
    RecoveryRejected {
        /// The pre-squash commit recovery attempted to restore.
        pre: String,
        /// The recovery command's nonzero exit code.
        exit_code: i32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosedFailure {
    CommitRejected { exit_code: i32 },
    ByteCheckFailed,
}

impl ClosedFailure {
    fn primary_detail(self) -> String {
        match self {
            Self::CommitRejected { exit_code } => format!("commit failed (exit {exit_code})"),
            Self::ByteCheckFailed => "BYTE-CHECK FAILED. Tree differed".into(),
        }
    }

    fn restored_detail(self, pre: &str) -> String {
        match self {
            Self::CommitRejected { exit_code } => {
                format!("commit failed (exit {exit_code}) — restored to {pre}")
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
    Rejected { exit_code: i32 },
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
    git: &impl GitRunner,
) -> Result<SquashResult, SquashLocalError> {
    let SquashLocal { repo, message, dry } = command;
    let output = git
        .run(&repo, &["rev-parse", "--show-toplevel"])
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() || output.stdout.trim().is_empty() {
        return Ok(SquashResult::new(SquashStatus::Refused, "not a git repo"));
    }
    let top = PathBuf::from(output.stdout.trim());

    let output = git
        .run(
            &top,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() || output.stdout.trim().is_empty() {
        return Ok(SquashResult::new(
            SquashStatus::Refused,
            "no upstream tracking branch (run: git push -u origin <branch>)",
        ));
    }
    let upstream = output.stdout.trim().to_string();

    if message.is_empty() {
        return Ok(SquashResult::new(
            SquashStatus::Refused,
            "commit message is required",
        ));
    }

    let range = format!("{upstream}..HEAD");
    let output = git
        .run(&top, &["rev-list", "--count", &range])
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() {
        return Ok(SquashResult::new(
            SquashStatus::Failed,
            format!("rev-list failed (exit {})", output.exit_code),
        ));
    }
    let Ok(count) = output.stdout.trim().parse::<usize>() else {
        return Ok(SquashResult::new(
            SquashStatus::Failed,
            "rev-list returned invalid count",
        ));
    };

    if count == 0 {
        return Ok(SquashResult {
            count,
            ..SquashResult::new(
                SquashStatus::Noop,
                format!("nothing unpushed (HEAD == {upstream})"),
            )
        });
    }

    let output = git
        .run(&top, &["log", "--format=%h %s", &range])
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() {
        return Ok(SquashResult {
            count,
            ..SquashResult::new(
                SquashStatus::Failed,
                format!("log failed (exit {})", output.exit_code),
            )
        });
    }
    let commits = lines(&output.stdout);

    if count == 1 {
        return Ok(SquashResult {
            count,
            commits,
            ..SquashResult::new(
                SquashStatus::Noop,
                "already one commit ahead — nothing to collapse",
            )
        });
    }

    if dry {
        return Ok(SquashResult {
            count,
            commits,
            ..SquashResult::new(
                SquashStatus::WouldSquash,
                format!("would collapse {count} commits into one"),
            )
        });
    }

    collapse(git, &top, &upstream, &message, count, commits)
}

fn collapse(
    git: &impl GitRunner,
    top: &Path,
    upstream: &str,
    message: &str,
    count: usize,
    commits: Vec<String>,
) -> Result<SquashResult, SquashLocalError> {
    let output = git
        .run(top, &["rev-parse", "HEAD"])
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() || output.stdout.trim().is_empty() {
        return Ok(SquashResult {
            count,
            commits,
            ..SquashResult::new(
                SquashStatus::Failed,
                format!("rev-parse HEAD failed (exit {})", output.exit_code),
            )
        });
    }
    let pre = output.stdout.trim().to_string();

    let output = git
        .run(top, &["reset", "--soft", upstream])
        .map_err(SquashLocalError::Unexpected)?;
    if !output.success() {
        return Ok(SquashResult {
            status: SquashStatus::Failed,
            count,
            commits,
            pre,
            detail: format!(
                "reset --soft failed (exit {}); no changes made.",
                output.exit_code
            ),
        });
    }

    let output = match git.run(top, &["commit", "-m", message]) {
        Ok(output) => output,
        Err(source) => {
            return Err(restore_after_transport_failure(git, top, &pre, source));
        }
    };
    if !output.success() {
        return restore_closed_failure(
            git,
            top,
            ClosedFailureContext {
                failure: ClosedFailure::CommitRejected {
                    exit_code: output.exit_code,
                },
                count,
                commits,
                pre,
            },
        );
    }

    let output = match git.run(top, &["diff", "--stat", &pre, "HEAD"]) {
        Ok(output) => output,
        Err(source) => {
            return Err(restore_after_transport_failure(git, top, &pre, source));
        }
    };
    if output.success() && output.stdout.trim().is_empty() {
        return Ok(SquashResult {
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
    git: &impl GitRunner,
    top: &Path,
    context: ClosedFailureContext,
) -> Result<SquashResult, SquashLocalError> {
    let ClosedFailureContext {
        failure,
        count,
        commits,
        pre,
    } = context;
    let detail = match restore(git, top, &pre) {
        RestoreOutcome::Restored => failure.restored_detail(&pre),
        RestoreOutcome::Rejected { exit_code } => {
            format!(
                "{}; recovery `git reset --soft {}` failed (exit {exit_code})",
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
    Ok(SquashResult {
        status: SquashStatus::Failed,
        count,
        commits,
        pre,
        detail,
    })
}

fn restore_after_transport_failure(
    git: &impl GitRunner,
    top: &Path,
    pre: &str,
    source: anyhow::Error,
) -> SquashLocalError {
    match restore(git, top, pre) {
        RestoreOutcome::Restored => SquashLocalError::Unexpected(source),
        RestoreOutcome::Rejected { exit_code } => SquashLocalError::RecoveryRejected {
            pre: pre.into(),
            exit_code,
            source,
        },
        RestoreOutcome::Transport(restore_source) => SquashLocalError::RecoveryTransport {
            primary: format!("{source:#}"),
            pre: pre.into(),
            source: restore_source,
        },
    }
}

fn restore(git: &impl GitRunner, top: &Path, pre: &str) -> RestoreOutcome {
    match git.run(top, &["reset", "--soft", pre]) {
        Ok(output) if output.success() => RestoreOutcome::Restored,
        Ok(output) => RestoreOutcome::Rejected {
            exit_code: output.exit_code,
        },
        Err(source) => RestoreOutcome::Transport(source),
    }
}

fn lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{SquashLocal, SquashResult, SquashStatus, execute};
    use crate::testing::FakeGitRunner;

    fn command(dry: bool) -> SquashLocal {
        SquashLocal {
            repo: ".".into(),
            message: "collapse".into(),
            dry,
        }
    }

    fn preflight(count: &str) -> Vec<crate::ports::GitOutput> {
        vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("origin/main\n"),
            FakeGitRunner::ok(count),
        ]
    }

    #[test]
    fn one_unpushed_commit_is_a_closed_noop_value() {
        let mut outputs = preflight("1\n");
        outputs.push(FakeGitRunner::ok("abc1234 one commit\n"));
        let git = FakeGitRunner::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashResult {
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
        let git = FakeGitRunner::new(preflight("not-a-count\n"));

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashResult {
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
            FakeGitRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeGitRunner::ok("pre123\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(" changed.txt | 1 +\n"),
            FakeGitRunner::ok(""),
        ]);
        let git = FakeGitRunner::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashResult {
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
            FakeGitRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeGitRunner::ok("pre123\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("commit rejected", 17),
            FakeGitRunner::ok(""),
        ]);
        let git = FakeGitRunner::new(outputs);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashResult {
                status: SquashStatus::Failed,
                count: 2,
                commits: vec!["abc1234 first".into(), "fed5678 second".into()],
                pre: "pre123".into(),
                detail: "commit failed (exit 17) — restored to pre123".into(),
            }
        );
    }

    #[test]
    fn commit_failure_preserves_primary_and_pre_when_restore_is_rejected() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(FakeGitRunner::ok("abc1234 first\nfed5678 second\n")),
            Ok(FakeGitRunner::ok("pre123\n")),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::exit_err("commit rejected", 17)),
            Ok(FakeGitRunner::exit_err("restore rejected", 23)),
        ]);
        let git = FakeGitRunner::with_results(results);

        assert_eq!(
            execute(command(false), &git).expect("Git transport should remain available"),
            SquashResult {
                status: SquashStatus::Failed,
                count: 2,
                commits: vec!["abc1234 first".into(), "fed5678 second".into()],
                pre: "pre123".into(),
                detail:
                    "commit failed (exit 17); recovery `git reset --soft pre123` failed (exit 23)"
                        .into(),
            }
        );
    }

    #[test]
    fn byte_check_failure_preserves_primary_and_pre_when_restore_transport_fails() {
        let mut results = preflight("2\n").into_iter().map(Ok).collect::<Vec<_>>();
        results.extend([
            Ok(FakeGitRunner::ok("abc1234 first\nfed5678 second\n")),
            Ok(FakeGitRunner::ok("pre123\n")),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok(" changed.txt | 1 +\n")),
            Err(anyhow::anyhow!("restore pipe closed").context("restore transport failed")),
        ]);
        let git = FakeGitRunner::with_results(results);

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
            Ok(FakeGitRunner::ok("abc1234 first\nfed5678 second\n")),
            Ok(FakeGitRunner::ok("pre123\n")),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("commit pipe closed").context("commit transport failed")),
            Ok(FakeGitRunner::exit_err("restore rejected", 23)),
        ]);
        let git = FakeGitRunner::with_results(results);

        let error =
            execute(command(false), &git).expect_err("primary transport must remain an error");

        assert_eq!(
            error.to_string(),
            "commit transport failed: commit pipe closed; recovery `git reset --soft pre123` failed (exit 23)"
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
            Ok(FakeGitRunner::ok("abc1234 first\nfed5678 second\n")),
            Ok(FakeGitRunner::ok("pre123\n")),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("commit pipe closed").context("commit transport failed")),
            Err(anyhow::anyhow!("restore pipe closed").context("restore transport failed")),
        ]);
        let git = FakeGitRunner::with_results(results);

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
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(command(false), &git).expect_err("transport failure must be an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
