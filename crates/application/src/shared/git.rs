//! Small combinators over the [`GitRunner`](crate::ports::GitRunner) port shared
//! by the local-git command flows and slices.

use std::path::Path;

use crate::ports::{GitOutput, GitRunner};

/// Formats Git argv as a concise command label for transport diagnostics.
pub(crate) fn command_label(args: &[&str]) -> String {
    format!("git {}", args.join(" "))
}

/// Runs git and returns trimmed stdout on a clean exit, else `None`.
pub fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    capture_checked(runner, repo, args).unwrap_or_default()
}

/// Runs a Git output probe while preserving unexpected transport failures.
pub(crate) fn capture_checked(
    runner: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> anyhow::Result<Option<String>> {
    let output = runner.run(repo, args)?;
    Ok(output.success().then(|| output.stdout.trim().to_string()))
}

/// Runs a Git boolean probe while preserving unexpected transport failures.
pub(crate) fn succeeds_checked(
    runner: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> anyhow::Result<bool> {
    runner.run(repo, args).map(|output| output.success())
}

/// Returns the last trimmed, non-empty line in captured process output.
pub fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

/// Extracts the abbreviated commit identity from a successful Git commit summary.
pub(crate) fn created_commit_identity(output: &GitOutput) -> Option<String> {
    output.stdout.lines().rev().find_map(|line| {
        let line = strip_ansi_csi(line);
        let (header, subject) = line.trim().strip_prefix('[')?.split_once(']')?;
        if subject.trim().is_empty() {
            return None;
        }
        let mut fields = header.split_whitespace();
        fields.next()?;
        let identity = fields.next_back()?;
        let length = identity.len();
        ((4..=64).contains(&length) && identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| identity.to_string())
    })
}

fn strip_ansi_csi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' || chars.next_if_eq(&'[').is_none() {
            output.push(character);
            continue;
        }
        for control_character in chars.by_ref() {
            if ('@'..='~').contains(&control_character) {
                break;
            }
        }
    }
    output
}

/// True when `<onto>` exists as a local branch.
pub fn onto_exists(runner: &impl GitRunner, repo: &Path, onto: &str) -> bool {
    onto_exists_checked(runner, repo, onto).unwrap_or(false)
}

/// Checks whether `<onto>` exists while preserving unexpected transport failures.
pub(crate) fn onto_exists_checked(
    runner: &impl GitRunner,
    repo: &Path,
    onto: &str,
) -> anyhow::Result<bool> {
    succeeds_checked(
        runner,
        repo,
        &["rev-parse", "--verify", &format!("refs/heads/{onto}")],
    )
}

#[cfg(test)]
mod tests {
    use super::{
        capture_checked, created_commit_identity, last_non_empty_line, onto_exists_checked,
        succeeds_checked,
    };
    use crate::{ports::GitOutput, testing::FakeGitRunner};

    #[test]
    fn last_non_empty_line_finds_the_last_populated_line() {
        assert_eq!(last_non_empty_line("a\n\nb\n \n"), Some("b"));
        assert_eq!(last_non_empty_line("\n\n"), None);
    }

    #[test]
    fn created_commit_identity_skips_noise_and_accepts_ansi_git_summary() {
        let output = |stdout: &str| crate::ports::GitOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: 0,
        };

        assert_eq!(
            created_commit_identity(&output(concat!(
                "[lint passed]\n",
                "\u{1b}[1m[\u{1b}[m\u{1b}[36mmain\u{1b}[m ",
                "\u{1b}[33mabc1234\u{1b}[m\u{1b}[1m]\u{1b}[m save\n",
            )))
            .as_deref(),
            Some("abc1234")
        );
        assert_eq!(
            created_commit_identity(&output("[main not-a-sha] save\n")),
            None
        );
        assert_eq!(
            created_commit_identity(&output("[deadbeef]\n[main abc1234] save\n")).as_deref(),
            Some("abc1234")
        );
        assert_eq!(
            created_commit_identity(&output("[lint abc1234]\n[main def5678] save\n")).as_deref(),
            Some("def5678")
        );
    }

    #[test]
    fn created_commit_identity_ignores_hook_diagnostics_on_stderr() {
        let output = GitOutput {
            stdout: "[lint abc1234]\n[main def5678] save\n".into(),
            stderr: "[hook deadbeef]\n".into(),
            exit_code: 0,
        };

        assert_eq!(created_commit_identity(&output).as_deref(), Some("def5678"));
        assert_eq!(
            created_commit_identity(&GitOutput {
                stdout: String::new(),
                stderr: "[hook deadbeef]\n".into(),
                exit_code: 0,
            }),
            None
        );
        assert_eq!(
            created_commit_identity(&GitOutput {
                stdout: "[hook deadbeef]\n".into(),
                stderr: String::new(),
                exit_code: 0,
            }),
            None
        );
    }

    #[test]
    fn checked_capture_distinguishes_rejection_from_transport() {
        let rejected = FakeGitRunner::new(vec![FakeGitRunner::exit_err("missing", 128)]);
        let unavailable =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        assert_eq!(
            capture_checked(&rejected, ".".as_ref(), &["rev-parse", "HEAD"])
                .expect("a nonzero exit is an expected probe rejection"),
            None
        );
        let error = capture_checked(&unavailable, ".".as_ref(), &["rev-parse", "HEAD"])
            .expect_err("transport failure must remain an error");
        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(error.source().map(ToString::to_string), None);
    }

    #[test]
    fn checked_success_distinguishes_false_from_transport() {
        let rejected = FakeGitRunner::new(vec![FakeGitRunner::exit_err("rejected", 1)]);
        let unavailable =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        assert!(
            !succeeds_checked(&rejected, ".".as_ref(), &["status"])
                .expect("a nonzero exit is an expected false result")
        );
        assert_eq!(
            succeeds_checked(&unavailable, ".".as_ref(), &["status"])
                .expect_err("transport failure must remain an error")
                .to_string(),
            "git transport unavailable"
        );
    }

    #[test]
    fn checked_branch_probe_distinguishes_missing_from_transport() {
        let missing = FakeGitRunner::new(vec![FakeGitRunner::exit_err("missing", 128)]);
        let unavailable =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        assert!(
            !onto_exists_checked(&missing, ".".as_ref(), "main")
                .expect("a missing branch is an expected false result")
        );
        assert_eq!(
            onto_exists_checked(&unavailable, ".".as_ref(), "main")
                .expect_err("transport failure must remain an error")
                .to_string(),
            "git transport unavailable"
        );
    }
}
