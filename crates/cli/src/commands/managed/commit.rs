//! Fanning `commit-all` out across every managed repo.

use serde::Serialize;

pub use super::working_tree::CommitFile;
use super::{
    ManagedExit, ManagedOptions, ManagedRepo, ManagedRun, git_capture::git_capture,
    push_pull::last_non_empty_line, working_tree,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitResult {
    pub name: String,
    pub present: bool,
    pub dirty: bool,
    pub files: Vec<CommitFile>,
    pub action: String,
    pub detail: String,
}

fn commit_exit_code(actions: &[&str], dry: bool) -> ManagedExit {
    if actions.contains(&"fail") {
        return ManagedExit::Fail;
    }
    if dry && actions.contains(&"would-commit") {
        return ManagedExit::Warn;
    }
    if !dry && actions.contains(&"skipped") {
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
            stderr: "commit-all: --message-for-all requires a non-empty message".to_string(),
        };
    }

    if !options.dry && options.message_for_all.is_none() && !options.interactive {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit-all: non-interactive shell; pass --dry or --message-for-all \"msg\""
                .to_string(),
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
                .map(|result| result.action.as_str())
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
    let state = working_tree::dirty_state(&repo.path);
    let mut result = CommitResult {
        name: repo.name.clone(),
        present: state.present,
        dirty: state.dirty,
        files: state.files,
        action: String::new(),
        detail: String::new(),
    };

    if !state.present {
        result.action = "absent".to_string();
        result.detail = "not present on this machine".to_string();
        return result;
    }
    if !state.dirty {
        result.action = "clean".to_string();
        result.detail = "nothing to commit".to_string();
        return result;
    }
    if options.dry {
        result.action = "would-commit".to_string();
        result.detail = format!("{} change(s)", result.files.len());
        return result;
    }

    let Some(message) = options.message_for_all.as_ref() else {
        result.action = "skipped".to_string();
        result.detail = "blank message - skipped".to_string();
        return result;
    };

    match git_capture(&repo.path, &["add", "-A"]) {
        Ok(output) if output.success() => {}
        _ => {
            result.action = "fail".to_string();
            result.detail = "git add failed".to_string();
            return result;
        }
    }

    match git_capture(&repo.path, &["commit", "-m", message]) {
        Ok(output) if output.success() => {
            result.action = "committed".to_string();
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("committed")
                .to_string();
        }
        Ok(output) => {
            result.action = "fail".to_string();
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("commit failed")
                .to_string();
        }
        Err(error) => {
            result.action = "fail".to_string();
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
    out.push_str(&format!("{:<30} {:<14} {}\n", "REPO", "ACTION", "DETAIL"));
    for result in results {
        out.push_str(&format!(
            "{:<30} {:<14} {}\n",
            result.name, result.action, result.detail
        ));
    }
    let actions = results
        .iter()
        .map(|result| result.action.as_str())
        .collect::<Vec<_>>();
    out.push_str(&format!(
        "\nexit {}  -  {} repos",
        commit_exit_code(&actions, dry).code(),
        results.len()
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::managed::test_support::{ManagedFixture, git_out};

    #[test]
    fn commit_exit_codes_match_dry_and_real_modes() {
        assert_eq!(
            commit_exit_code(&["clean", "absent"], true),
            ManagedExit::Clean
        );
        assert_eq!(commit_exit_code(&["would-commit"], true), ManagedExit::Warn);
        assert_eq!(commit_exit_code(&["skipped"], false), ManagedExit::Warn);
        assert_eq!(commit_exit_code(&["fail"], false), ManagedExit::Fail);
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
        assert_eq!(run.results[0].action, "would-commit");
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
        assert_eq!(run.results[0].action, "committed");
        assert_eq!(git_out(&repo, &["status", "--porcelain"]), "");
    }
}
