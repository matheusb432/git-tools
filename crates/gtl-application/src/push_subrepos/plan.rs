//! The `push_subrepos/plan` query: discover every git repo under a root through
//! the [`RepoDiscovery`] port and resolve each one's push destination from local
//! refs through the [`GitClient`] port — read-only, never fetches or pushes.

use std::path::{Path, PathBuf};

use gtl_models::managed::push_subrepos::{Dest, RepoTarget, SubreposPlan};

use crate::{
    discovery::find_repos,
    ports::{GitClient, RepoDiscovery},
};

/// Plan a recursive push of every git repo under `root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPush {
    pub root: PathBuf,
}

/// Everything that can go wrong planning a recursive push.
#[derive(Debug, thiserror::Error)]
pub enum PlanPushError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Discovers every git repo under the root and resolves each one's push destination.
/// Linked worktrees are skipped (via the `discovery` slice).
#[cqrsy::query]
pub fn execute(
    req: PlanPush,
    discovery: &impl RepoDiscovery,
    git: &impl GitClient,
) -> Result<SubreposPlan, PlanPushError> {
    let PlanPush { root } = req;
    let discovered = find_repos::execute(
        find_repos::DiscoverRepos {
            root: root.clone(),
            include_worktrees: false,
        },
        discovery,
    )
    .map_err(anyhow::Error::from)?;
    if discovered.is_empty() {
        return Ok(SubreposPlan::Refused(format!(
            "no git repos found under {}",
            root.display()
        )));
    }
    let targets = discovered
        .into_iter()
        .map(|repo| inspect(git, &repo.path, repo.label))
        .collect();
    Ok(SubreposPlan::Ready(targets))
}

/// Resolves one repo's push destination from local refs only — never fetches. A detached
/// HEAD or a branch with no `branch.<name>.remote` becomes a [`Dest::Skip`]; a branch with
/// no unpushed commits becomes a [`Dest::Synced`] so the push is skipped entirely.
fn inspect(git: &impl GitClient, path: &Path, label: String) -> RepoTarget {
    let dest = match git.current_branch(path).ok() {
        Some(branch) if branch != "HEAD" => match git.branch_remote(path, &branch).ok().flatten() {
            Some(remote) if is_synced(git, path) => Dest::Synced { branch, remote },
            Some(remote) => Dest::Push { branch, remote },
            None => Dest::Skip {
                reason: "no upstream tracking branch".to_string(),
            },
        },
        _ => Dest::Skip {
            reason: "detached HEAD".to_string(),
        },
    };
    RepoTarget {
        path: path.to_path_buf(),
        label,
        dest,
    }
}

/// Whether the current branch has no unpushed commits — the local `@{u}..HEAD` count is
/// exactly `0`. This mirrors the ahead-count `gtl status --all` reports and the sibling
/// `diff -r` uses to spot synced repos, and stays purely local (no fetch). An unavailable
/// count (e.g. no `@{u}` merge ref) is never read as synced — we fall back to pushing.
fn is_synced(git: &impl GitClient, path: &Path) -> bool {
    git.commit_count(path, "@{u}..HEAD").ok().flatten() == Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeRepoDiscovery, ScriptedGitClient};

    #[test]
    fn inspect_resolves_push_when_branch_is_ahead() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::applied("2\n"), /* rev-list --count @{u}..HEAD — two unpushed
                                                * commits */
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
    }

    #[test]
    fn inspect_marks_synced_when_no_unpushed_commits() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::applied("0\n"), // rev-list --count @{u}..HEAD — nothing to push
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
    }

    #[test]
    fn inspect_pushes_when_ahead_count_is_unavailable() {
        // A failed `rev-list --count` must never read as "0 / already synced" — fall back to
        // attempting the push, exactly as the sibling checks do (`sw`, `diff -r`).
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::rejected(""),
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
    }

    #[test]
    fn inspect_skips_branch_without_upstream() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("feat\n"),
            ScriptedGitClient::rejected(""),
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "no upstream tracking branch".into(),
            }
        );
    }

    #[test]
    fn inspect_skips_detached_head() {
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::applied("HEAD\n")]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "detached HEAD".into(),
            }
        );
    }

    #[test]
    fn plan_refuses_when_no_repos_are_discovered() {
        let plan = execute(
            PlanPush {
                root: "/work".into(),
            },
            &FakeRepoDiscovery::default(),
            &ScriptedGitClient::default(),
        )
        .expect("planning succeeds");

        assert_eq!(
            plan,
            SubreposPlan::Refused("no git repos found under /work".into())
        );
    }

    #[test]
    fn plan_inspects_every_discovered_repo_with_its_label() {
        let discovery = FakeRepoDiscovery {
            repos: vec!["/work/api".into(), "/work/libs/inner".into()],
        };
        // Both repos detached: one scripted rev-parse per repo keeps the fixture minimal.
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        let plan = execute(
            PlanPush {
                root: "/work".into(),
            },
            &discovery,
            &runner,
        )
        .expect("planning succeeds");

        let SubreposPlan::Ready(targets) = plan else {
            panic!("expected a ready plan, got {plan:?}");
        };
        let labels: Vec<&str> = targets.iter().map(|target| target.label.as_str()).collect();
        assert_eq!(labels, ["api", "libs/inner"]);
    }
}
