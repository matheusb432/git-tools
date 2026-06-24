use std::path::Path;
use std::process::Command;

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    /// git's stderr (where it writes diagnostics on failure). Empty unless captured.
    pub stderr: String,
    pub exit_code: i32,
}

impl GitOutput {
    fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// git's own diagnostic — stderr (where it writes errors) if present, else stdout —
    /// trimmed. Empty when git said nothing.
    pub fn diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.stdout.trim()
        } else {
            stderr
        }
    }

    /// A failure detail: `context` plus git's own message when it gave one, otherwise
    /// `context (exit N)`. Lets every command surface git's real reason uniformly.
    pub fn fail_detail(&self, context: &str) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => format!("{context}: {said}"),
            _ => format!("{context} (exit {})", self.exit_code),
        }
    }
}

pub trait GitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdGitRunner;

impl GitRunner for StdGitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .with_context(|| format!("failed to run git in {}", repo.display()))?;

        Ok(GitOutput {
            stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(1),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Refused,
    Noop,
    WouldSquash,
    Squashed,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashResult {
    pub status: Status,
    pub count: usize,
    pub commits: Vec<String>,
    pub pre: String,
    pub detail: String,
}

impl SquashResult {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            count: 0,
            commits: Vec::new(),
            pre: String::new(),
            detail: detail.into(),
        }
    }
}

pub fn invoke_squash_local(
    runner: &impl GitRunner,
    repo: impl AsRef<Path>,
    message: Option<&str>,
    dry: bool,
) -> SquashResult {
    let top = match run_git(runner, repo.as_ref(), &["rev-parse", "--show-toplevel"]) {
        Ok(output) if output.success() && !output.stdout.trim().is_empty() => {
            output.stdout.trim().to_string()
        }
        _ => return SquashResult::new(Status::Refused, "not a git repo"),
    };
    let top_path = Path::new(&top);

    let upstream = match run_git(
        runner,
        top_path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) {
        Ok(output) if output.success() && !output.stdout.trim().is_empty() => {
            output.stdout.trim().to_string()
        }
        _ => {
            return SquashResult::new(
                Status::Refused,
                "no upstream tracking branch (run: git push -u origin <branch>)",
            );
        }
    };

    let message = match message.filter(|message| !message.is_empty()) {
        Some(message) => message,
        None => return SquashResult::new(Status::Refused, "commit message is required"),
    };

    let range = format!("{upstream}..HEAD");
    let count = match run_git(runner, top_path, &["rev-list", "--count", &range]) {
        Ok(output) if output.success() => match output.stdout.trim().parse::<usize>() {
            Ok(count) => count,
            Err(_) => return SquashResult::new(Status::Fail, "rev-list returned invalid count"),
        },
        Ok(output) => {
            return SquashResult::new(
                Status::Fail,
                format!("rev-list failed (exit {})", output.exit_code),
            );
        }
        Err(error) => return SquashResult::new(Status::Fail, error.to_string()),
    };

    if count == 0 {
        return SquashResult {
            count,
            ..SquashResult::new(
                Status::Noop,
                format!("nothing unpushed (HEAD == {upstream})"),
            )
        };
    }

    let commits = match run_git(runner, top_path, &["log", "--format=%h %s", &range]) {
        Ok(output) if output.success() => lines(&output.stdout),
        Ok(output) => {
            return SquashResult {
                count,
                ..SquashResult::new(
                    Status::Fail,
                    format!("log failed (exit {})", output.exit_code),
                )
            };
        }
        Err(error) => {
            return SquashResult {
                count,
                ..SquashResult::new(Status::Fail, error.to_string())
            };
        }
    };

    if count == 1 {
        return SquashResult {
            count,
            commits,
            ..SquashResult::new(
                Status::Noop,
                "already one commit ahead — nothing to collapse",
            )
        };
    }

    if dry {
        return SquashResult {
            count,
            commits,
            ..SquashResult::new(
                Status::WouldSquash,
                format!("would collapse {count} commits into one"),
            )
        };
    }

    let pre = match run_git(runner, top_path, &["rev-parse", "HEAD"]) {
        Ok(output) if output.success() && !output.stdout.trim().is_empty() => {
            output.stdout.trim().to_string()
        }
        Ok(output) => {
            return SquashResult {
                count,
                commits,
                ..SquashResult::new(
                    Status::Fail,
                    format!("rev-parse HEAD failed (exit {})", output.exit_code),
                )
            };
        }
        Err(error) => {
            return SquashResult {
                count,
                commits,
                ..SquashResult::new(Status::Fail, error.to_string())
            };
        }
    };

    match run_git(runner, top_path, &["reset", "--soft", &upstream]) {
        Ok(output) if output.success() => {}
        Ok(output) => {
            return SquashResult {
                count,
                commits,
                pre,
                detail: format!(
                    "reset --soft failed (exit {}); no changes made.",
                    output.exit_code
                ),
                status: Status::Fail,
            };
        }
        Err(_) => {
            return SquashResult {
                count,
                commits,
                pre,
                detail: "reset --soft failed (exit 1); no changes made.".to_string(),
                status: Status::Fail,
            };
        }
    }

    match run_git(runner, top_path, &["commit", "-m", message]) {
        Ok(output) if output.success() => {}
        Ok(output) => {
            let _ = run_git(runner, top_path, &["reset", "--soft", &pre]);
            return SquashResult {
                count,
                commits,
                pre: pre.clone(),
                detail: format!(
                    "commit failed (exit {}) — restored to {pre}",
                    output.exit_code
                ),
                status: Status::Fail,
            };
        }
        Err(_) => {
            let _ = run_git(runner, top_path, &["reset", "--soft", &pre]);
            return SquashResult {
                count,
                commits,
                pre: pre.clone(),
                detail: format!("commit failed (exit 1) — restored to {pre}"),
                status: Status::Fail,
            };
        }
    }

    match run_git(runner, top_path, &["diff", "--stat", &pre, "HEAD"]) {
        Ok(output) if output.success() && output.stdout.trim().is_empty() => SquashResult {
            count,
            commits,
            pre: pre.clone(),
            detail: format!(
                "collapsed {count} commits into one. recover with: git reset --soft {pre}"
            ),
            status: Status::Squashed,
        },
        Ok(_) => {
            let _ = run_git(runner, top_path, &["reset", "--soft", &pre]);
            SquashResult {
                count,
                commits,
                pre: pre.clone(),
                detail: format!(
                    "BYTE-CHECK FAILED — restored to {pre}. Tree differed; nothing changed."
                ),
                status: Status::Fail,
            }
        }
        Err(error) => {
            let _ = run_git(runner, top_path, &["reset", "--soft", &pre]);
            SquashResult {
                count,
                commits,
                pre,
                detail: error.to_string(),
                status: Status::Fail,
            }
        }
    }
}

fn run_git(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
    runner.run(repo, args)
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
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use anyhow::anyhow;

    use super::{GitOutput, GitRunner, SquashResult, Status, invoke_squash_local};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        repo: PathBuf,
        args: Vec<String>,
    }

    #[derive(Debug, Clone)]
    enum Scripted {
        Output(GitOutput),
        Error(&'static str),
    }

    struct FakeRunner {
        calls: RefCell<Vec<Call>>,
        results: RefCell<Vec<Scripted>>,
    }

    impl FakeRunner {
        fn new(results: Vec<Scripted>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                results: RefCell::new(results),
            }
        }

        fn ok(stdout: &str) -> Scripted {
            Scripted::Output(GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code: 0,
            })
        }

        fn exit(stdout: &str, exit_code: i32) -> Scripted {
            Scripted::Output(GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code,
            })
        }

        fn err(message: &'static str) -> Scripted {
            Scripted::Error(message)
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.borrow().clone()
        }
    }

    impl GitRunner for FakeRunner {
        fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls.borrow_mut().push(Call {
                repo: repo.to_path_buf(),
                args: args.iter().map(|arg| arg.to_string()).collect(),
            });
            match self.results.borrow_mut().remove(0) {
                Scripted::Output(output) => Ok(output),
                Scripted::Error(message) => Err(anyhow!(message)),
            }
        }
    }

    fn successful_preamble(count: usize) -> Vec<Scripted> {
        vec![
            FakeRunner::ok("C:/repo\n"),
            FakeRunner::ok("origin/main\n"),
            FakeRunner::ok(&format!("{count}\n")),
        ]
    }

    fn assert_result(
        result: SquashResult,
        status: Status,
        count: usize,
        commits: &[&str],
        pre: &str,
        detail: &str,
    ) {
        assert_eq!(result.status, status);
        assert_eq!(result.count, count);
        assert_eq!(result.commits, commits);
        assert_eq!(result.pre, pre);
        assert_eq!(result.detail, detail);
    }

    #[test]
    fn refuses_when_not_a_git_repo() {
        let runner = FakeRunner::new(vec![FakeRunner::exit("", 128)]);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(result, Status::Refused, 0, &[], "", "not a git repo");
        assert_eq!(
            runner.calls(),
            vec![Call {
                repo: PathBuf::from("C:/repo"),
                args: vec!["rev-parse".into(), "--show-toplevel".into()],
            }]
        );
    }

    #[test]
    fn refuses_when_no_upstream() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("C:/repo\n"), FakeRunner::exit("", 128)]);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Refused,
            0,
            &[],
            "",
            "no upstream tracking branch (run: git push -u origin <branch>)",
        );
    }

    #[test]
    fn refuses_when_message_is_missing_after_repo_checks() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("C:/repo\n"),
            FakeRunner::ok("origin/main\n"),
        ]);

        let result = invoke_squash_local(&runner, "C:/repo", None, false);

        assert_result(
            result,
            Status::Refused,
            0,
            &[],
            "",
            "commit message is required",
        );
        assert_eq!(runner.calls().len(), 2);
    }

    #[test]
    fn noops_when_head_matches_upstream() {
        let runner = FakeRunner::new(successful_preamble(0));

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Noop,
            0,
            &[],
            "",
            "nothing unpushed (HEAD == origin/main)",
        );
    }

    #[test]
    fn noops_when_already_one_commit_ahead() {
        let mut script = successful_preamble(1);
        script.push(FakeRunner::ok("abc1234 one commit\n"));
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Noop,
            1,
            &["abc1234 one commit"],
            "",
            "already one commit ahead — nothing to collapse",
        );
    }

    #[test]
    fn dry_run_would_squash_two_or_more_commits() {
        let mut script = successful_preamble(2);
        script.push(FakeRunner::ok("abc1234 first\nfed5678 second\n"));
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), true);

        assert_result(
            result,
            Status::WouldSquash,
            2,
            &["abc1234 first", "fed5678 second"],
            "",
            "would collapse 2 commits into one",
        );
    }

    #[test]
    fn squashes_commits_when_byte_check_is_clean() {
        let mut script = successful_preamble(3);
        script.extend([
            FakeRunner::ok("abc1234 first\nfed5678 second\n987abcd third\n"),
            FakeRunner::ok("pre123\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
        ]);
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Squashed,
            3,
            &["abc1234 first", "fed5678 second", "987abcd third"],
            "pre123",
            "collapsed 3 commits into one. recover with: git reset --soft pre123",
        );
        let calls = runner.calls();
        assert_eq!(calls[4].args, vec!["rev-parse", "HEAD"]);
        assert_eq!(
            calls[5].args,
            vec!["reset", "--soft", "origin/main"],
            "reset must happen before commit"
        );
        assert_eq!(
            calls[6].args,
            vec!["commit", "-m", "collapse"],
            "commit must follow reset"
        );
        assert_eq!(
            calls[7].args,
            vec!["diff", "--stat", "pre123", "HEAD"],
            "byte-check must run after commit"
        );
    }

    #[test]
    fn restores_pre_squash_head_when_byte_check_fails() {
        let mut script = successful_preamble(2);
        script.extend([
            FakeRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeRunner::ok("pre123\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::ok(" changed.txt | 1 +\n"),
            FakeRunner::ok(""),
        ]);
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Fail,
            2,
            &["abc1234 first", "fed5678 second"],
            "pre123",
            "BYTE-CHECK FAILED — restored to pre123. Tree differed; nothing changed.",
        );
        let calls = runner.calls();
        assert_eq!(calls[4].args, vec!["rev-parse", "HEAD"]);
        assert_eq!(calls[5].args, vec!["reset", "--soft", "origin/main"]);
        assert_eq!(calls[6].args, vec!["commit", "-m", "collapse"]);
        assert_eq!(calls[7].args, vec!["diff", "--stat", "pre123", "HEAD"]);
        assert_eq!(calls[8].args, vec!["reset", "--soft", "pre123"]);
    }

    #[test]
    fn reset_soft_failure_returns_fail_without_attempting_commit() {
        let mut script = successful_preamble(2);
        script.extend([
            FakeRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeRunner::ok("pre123\n"),
            FakeRunner::exit("", 13),
        ]);
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Fail,
            2,
            &["abc1234 first", "fed5678 second"],
            "pre123",
            "reset --soft failed (exit 13); no changes made.",
        );
        let calls = runner.calls();
        assert_eq!(calls[5].args, vec!["reset", "--soft", "origin/main"]);
        assert!(
            calls
                .iter()
                .all(|call| call.args.first().map(String::as_str) != Some("commit")),
            "commit must not be attempted after reset failure"
        );
    }

    #[test]
    fn commit_failure_restores_pre_squash_head() {
        let mut script = successful_preamble(2);
        script.extend([
            FakeRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeRunner::ok("pre123\n"),
            FakeRunner::ok(""),
            FakeRunner::exit("", 17),
            FakeRunner::ok(""),
        ]);
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Fail,
            2,
            &["abc1234 first", "fed5678 second"],
            "pre123",
            "commit failed (exit 17) — restored to pre123",
        );
        let calls = runner.calls();
        assert_eq!(calls[5].args, vec!["reset", "--soft", "origin/main"]);
        assert_eq!(calls[6].args, vec!["commit", "-m", "collapse"]);
        assert_eq!(calls[7].args, vec!["reset", "--soft", "pre123"]);
    }

    #[test]
    fn byte_check_runner_error_attempts_restore_before_returning_fail() {
        let mut script = successful_preamble(2);
        script.extend([
            FakeRunner::ok("abc1234 first\nfed5678 second\n"),
            FakeRunner::ok("pre123\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::err("diff failed"),
            FakeRunner::ok(""),
        ]);
        let runner = FakeRunner::new(script);

        let result = invoke_squash_local(&runner, "C:/repo", Some("collapse"), false);

        assert_result(
            result,
            Status::Fail,
            2,
            &["abc1234 first", "fed5678 second"],
            "pre123",
            "diff failed",
        );
        let calls = runner.calls();
        assert_eq!(calls[5].args, vec!["reset", "--soft", "origin/main"]);
        assert_eq!(calls[6].args, vec!["commit", "-m", "collapse"]);
        assert_eq!(calls[7].args, vec!["diff", "--stat", "pre123", "HEAD"]);
        assert_eq!(calls[8].args, vec!["reset", "--soft", "pre123"]);
    }
}
