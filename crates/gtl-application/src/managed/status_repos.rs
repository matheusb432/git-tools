//! The `managed/status_repos` query: classify each repo's status (branch,
//! upstream/ahead, dirty counts) through the [`GitClient`] port. The repo list
//! comes from the caller — the manifest for `status --all`, the discovery slice
//! for `status -r`, the enclosing repo for plain `status` — so the classify
//! rules live exactly once.

use std::path::Path;

use gtl_models::managed::{ManagedRepo, status::StatusResult};

use crate::ports::GitClient;

/// Classify the status of every listed repo, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRepos {
    pub repos: Vec<ManagedRepo>,
}

/// Classifies each repo purely from local refs (no fetch). Infallible by design:
/// an unreadable fact degrades to its neutral value (`branch-unavailable`,
/// `no-upstream`, ahead `0`) and is reported in the result, never as an error.
#[cqrsy::query]
pub fn execute(query: StatusRepos, git: &impl GitClient) -> Vec<StatusResult> {
    let StatusRepos { repos } = query;
    repos.iter().map(|repo| status_one(git, repo)).collect()
}

fn status_one(git: &impl GitClient, repo: &ManagedRepo) -> StatusResult {
    let mut result = StatusResult {
        name: repo.name.clone(),
        present: false,
        branch: String::new(),
        upstream: String::new(),
        ahead: 0,
        dirty: false,
        dirty_count: 0,
        untracked_count: 0,
        state: "absent".to_string(),
        detail: "not present".to_string(),
    };

    if !git.repo_present(&repo.path) {
        return result;
    }

    result.present = true;
    result.branch = git.current_branch(&repo.path).unwrap_or_default();

    let dirty = super::logic::working_tree::dirty_state(git, &repo.path);
    result.untracked_count = dirty
        .files
        .iter()
        .filter(|file| file.status == "??")
        .count();
    result.dirty_count = dirty.files.len().saturating_sub(result.untracked_count);
    result.dirty = result.dirty_count > 0 || result.untracked_count > 0;

    let mut parts = Vec::new();
    if result.branch.is_empty() {
        parts.push("branch-unavailable".to_string());
    } else if result.branch == "HEAD" {
        result.branch = "detached".to_string();
        parts.push("detached".to_string());
    } else {
        match upstream_ahead(git, &repo.path) {
            Some((upstream, ahead)) => {
                result.upstream = upstream;
                result.ahead = ahead;
                if ahead > 0 {
                    parts.push(format!("⇡{ahead}"));
                }
            }
            None => parts.push("no-upstream".to_string()),
        }
    }

    let change_symbols = format!(
        "{}{}",
        if result.dirty_count > 0 { "!" } else { "" },
        if result.untracked_count > 0 { "?" } else { "" }
    );
    if !change_symbols.is_empty() {
        parts.push(change_symbols);
    }
    if parts.is_empty() {
        parts.push("✓".to_string());
    }

    result.detail = parts.join(" ");
    result.state = if result.detail == "✓" {
        "clean".to_string()
    } else if result.detail.contains("no-upstream")
        || result.detail.contains("detached")
        || result.detail.contains("branch-unavailable")
    {
        "warn".to_string()
    } else {
        "pending".to_string()
    };
    result
}

/// The current branch's upstream tracking ref and how many commits it is ahead of that ref
/// (`@{u}..HEAD`) — the two sync facts `gtl status --all` reports. `None` when the branch has
/// no upstream. Purely local (no fetch); the same semantics `push --all` uses to skip repos
/// already synced with their remote instead of pushing every one.
fn upstream_ahead(git: &impl GitClient, repo_path: &Path) -> Option<(String, usize)> {
    let crate::ports::GitEffect::Applied(upstream) = git.upstream(repo_path).ok()? else {
        return None;
    };
    let ahead = git
        .commit_count(repo_path, "@{u}..HEAD")
        .ok()
        .flatten()
        .unwrap_or(0);
    Some((upstream, ahead))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::testing::ScriptedGitClient;

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.to_string(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: String::new(),
        }
    }

    #[test]
    fn absent_repo_classifies_without_calling_git() {
        let runner = ScriptedGitClient::default();
        runner
            .absent_repos
            .lock()
            .unwrap()
            .push("/repos/api".into());

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].state, "absent");
        assert_eq!(results[0].detail, "not present");
        assert!(!results[0].present);
    }

    #[test]
    fn clean_synced_repo_classifies_as_clean_checkmark() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"), // rev-parse --abbrev-ref HEAD
            ScriptedGitClient::applied(""),       // status --porcelain (clean)
            ScriptedGitClient::applied("origin/main\n"), // rev-parse @{u}
            ScriptedGitClient::applied("0\n"),    // rev-list --count @{u}..HEAD
        ]);

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].state, "clean");
        assert_eq!(results[0].detail, "✓");
        assert_eq!(results[0].upstream, "origin/main");
        assert_eq!(results[0].ahead, 0);
    }

    #[test]
    fn ahead_and_dirty_repo_classifies_as_pending_with_markers() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(" M src/lib.rs\n?? new.txt\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
        ]);

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].state, "pending");
        assert_eq!(results[0].detail, "⇡2 !?");
        assert_eq!(results[0].ahead, 2);
        assert_eq!(results[0].dirty_count, 1);
        assert_eq!(results[0].untracked_count, 1);
        assert!(results[0].dirty);
    }

    #[test]
    fn detached_head_classifies_as_warn() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied(""),
        ]);

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].state, "warn");
        assert_eq!(results[0].branch, "detached");
        assert_eq!(results[0].detail, "detached");
    }

    #[test]
    fn missing_upstream_classifies_as_warn() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("feat\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("fatal: no upstream"),
        ]);

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].state, "warn");
        assert_eq!(results[0].detail, "no-upstream");
        assert_eq!(results[0].upstream, "");
    }

    #[test]
    fn unavailable_ahead_count_degrades_to_zero() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::rejected("boom"),
        ]);

        let results = execute(
            StatusRepos {
                repos: vec![repo("api")],
            },
            &runner,
        );

        assert_eq!(results[0].ahead, 0);
        assert_eq!(results[0].state, "clean");
    }
}
