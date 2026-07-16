//! Fanning `commit --all` out across every managed repo.

use std::fmt::{self, Write as _};

use application::{managed::working_tree, ports::GitRunner as _};
pub use domain::managed::working_tree::CommitFile;
use infra::git_runner::StdGitRunner;
use serde::{Serialize, Serializer};

use super::{ManagedExit, ManagedOptions, ManagedRepo, ManagedRun, push_pull::last_non_empty_line};

/// What `commit --all` did with one repo. Replaces the former stringly-typed
/// `action` so [`commit_exit_code`] and every call site are checked against the
/// closed set. [`CommitAction::as_wire`] is the exact token the `--json` output
/// and the action table have always emitted — keep it byte-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitAction {
    /// Repo not present on this machine.
    Absent,
    /// Nothing to commit — working tree clean.
    Clean,
    /// Dirty changes a real run would commit (dry-run).
    WouldCommit,
    /// Dirty but skipped (no message supplied).
    Skipped,
    /// Changes were committed.
    Committed,
    /// The commit attempt failed.
    Fail,
}

impl CommitAction {
    /// The stable wire/display token for this action.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Clean => "clean",
            Self::WouldCommit => "would-commit",
            Self::Skipped => "skipped",
            Self::Committed => "committed",
            Self::Fail => "fail",
        }
    }
}

impl fmt::Display for CommitAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

impl Serialize for CommitAction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitResult {
    pub name: String,
    pub present: bool,
    pub dirty: bool,
    pub files: Vec<CommitFile>,
    pub action: CommitAction,
    pub detail: String,
}

fn commit_exit_code(actions: &[CommitAction], dry: bool) -> ManagedExit {
    if actions.contains(&CommitAction::Fail) {
        return ManagedExit::Fail;
    }
    if dry && actions.contains(&CommitAction::WouldCommit) {
        return ManagedExit::Warn;
    }
    if !dry && actions.contains(&CommitAction::Skipped) {
        return ManagedExit::Warn;
    }
    ManagedExit::Clean
}

pub fn run_commit_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    if let Some(message) = &options.message_for_all
        && message.trim().is_empty()
    {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit --all requires a non-empty message".to_string(),
        };
    }

    if !options.dry && options.message_for_all.is_none() && !options.interactive {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit --all requires a message unless --dry is used".to_string(),
        };
    }

    match super::manifest::load_repos(options) {
        Ok(repos) => {
            let results = repos
                .iter()
                .map(|repo| commit_one(repo, options))
                .collect::<Vec<_>>();
            let actions = results
                .iter()
                .map(|result| result.action)
                .collect::<Vec<_>>();
            let exit = commit_exit_code(&actions, options.dry);
            let stdout = format_commit(options.dry, options.json, &results);
            ManagedRun {
                exit,
                results,
                stdout,
                stderr: String::new(),
            }
        }
        Err(error) => ManagedRun {
            exit: ManagedExit::Fail,
            results: Vec::new(),
            stdout: String::new(),
            stderr: format!("{error:#}"),
        },
    }
}

fn commit_one(repo: &ManagedRepo, options: &ManagedOptions) -> CommitResult {
    let state = working_tree::dirty_state(&StdGitRunner, &repo.path);
    let mut result = CommitResult {
        name: repo.name.clone(),
        present: state.present,
        dirty: state.dirty,
        files: state.files,
        action: CommitAction::Clean,
        detail: String::new(),
    };

    if !state.present {
        result.action = CommitAction::Absent;
        result.detail = "not present on this machine".to_string();
        return result;
    }
    if !state.dirty {
        result.action = CommitAction::Clean;
        result.detail = "nothing to commit".to_string();
        return result;
    }
    if options.dry {
        result.action = CommitAction::WouldCommit;
        result.detail = format!("{} change(s)", result.files.len());
        return result;
    }

    let Some(message) = options.message_for_all.as_ref() else {
        result.action = CommitAction::Skipped;
        result.detail = "blank message - skipped".to_string();
        return result;
    };

    match StdGitRunner.run(&repo.path, &["add", "-A"]) {
        Ok(output) if output.success() => {}
        _ => {
            result.action = CommitAction::Fail;
            result.detail = "git add failed".to_string();
            return result;
        }
    }

    match StdGitRunner.run(&repo.path, &["commit", "-m", message]) {
        Ok(output) if output.success() => {
            result.action = CommitAction::Committed;
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("committed")
                .to_string();
        }
        Ok(output) => {
            result.action = CommitAction::Fail;
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("commit failed")
                .to_string();
        }
        Err(error) => {
            result.action = CommitAction::Fail;
            result.detail = error.to_string();
        }
    }
    result
}

fn format_commit(dry: bool, json: bool, results: &[CommitResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let mut out = String::new();
    let _ = writeln!(out, "{:<30} {:<14} DETAIL", "REPO", "ACTION");
    for result in results {
        let _ = writeln!(
            out,
            "{:<30} {:<14} {}",
            result.name, result.action, result.detail
        );
    }
    let actions = results
        .iter()
        .map(|result| result.action)
        .collect::<Vec<_>>();
    let _ = write!(
        out,
        "\nexit {}  -  {} repos",
        commit_exit_code(&actions, dry).code(),
        results.len()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::managed::test_support::{ManagedFixture, git_out};

    #[test]
    fn commit_action_wire_tokens_are_byte_stable() {
        // The `--json` `Action` field and the action-table column emit these tokens;
        // changing one is a breaking output change, so pin every variant.
        assert_eq!(CommitAction::Absent.as_wire(), "absent");
        assert_eq!(CommitAction::Clean.as_wire(), "clean");
        assert_eq!(CommitAction::WouldCommit.as_wire(), "would-commit");
        assert_eq!(CommitAction::Skipped.as_wire(), "skipped");
        assert_eq!(CommitAction::Committed.as_wire(), "committed");
        assert_eq!(CommitAction::Fail.as_wire(), "fail");
        // Serialize routes through as_wire, so JSON stays identical to the old String.
        assert_eq!(
            serde_json::to_string(&CommitAction::WouldCommit).unwrap(),
            "\"would-commit\""
        );
    }

    #[test]
    fn commit_exit_codes_match_dry_and_real_modes() {
        assert_eq!(
            commit_exit_code(&[CommitAction::Clean, CommitAction::Absent], true),
            ManagedExit::Clean
        );
        assert_eq!(
            commit_exit_code(&[CommitAction::WouldCommit], true),
            ManagedExit::Warn
        );
        assert_eq!(
            commit_exit_code(&[CommitAction::Skipped], false),
            ManagedExit::Warn
        );
        assert_eq!(
            commit_exit_code(&[CommitAction::Fail], false),
            ManagedExit::Fail
        );
    }

    #[test]
    fn commit_all_dry_reports_dirty_without_committing() {
        let fixture = ManagedFixture::new("commit-dry");
        let repo = fixture.init_repo("repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file("repo/work.txt", "dirty\n");
        let before = git_out(&repo, &["rev-parse", "HEAD"]);

        let run = run_commit_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: true,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Warn);
        assert_eq!(run.results[0].action, CommitAction::WouldCommit);
        assert_eq!(git_out(&repo, &["rev-parse", "HEAD"]), before);
    }

    #[test]
    fn commit_all_message_for_all_commits_dirty_repo() {
        let fixture = ManagedFixture::new("commit-batch");
        let repo = fixture.init_repo("repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file("repo/work.txt", "dirty\n");

        let run = run_commit_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: false,
            json: false,
            color: false,
            message_for_all: Some("save work".to_string()),
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].action, CommitAction::Committed);
        assert_eq!(git_out(&repo, &["status", "--porcelain"]), "");
    }
}
