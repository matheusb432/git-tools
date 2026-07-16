//! The `discovery/find_repo_tops` query: discover every git repo under a root
//! (via [`find_repos`]) and resolve each to its canonical top-level path through
//! the [`GitRunner`](crate::ports::GitRunner) port — the unit the `diff -r` wire
//! paths hand to the daemon.

use std::path::{Path, PathBuf};

use domain::discovery::DiscoveredRepo;

use crate::{
    discovery::find_repos,
    ports::{GitRunner, RepoDiscovery},
};

/// Discover every git repo under `root`, resolved to its canonical top level and
/// labeled relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindRepoTops {
    pub root: PathBuf,
    pub include_worktrees: bool,
}

/// Everything that can go wrong discovering and resolving repo tops.
#[derive(Debug, thiserror::Error)]
pub enum FindRepoTopsError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Walk `root` through the discovery slice and resolve each repo's `path` to its
/// `git rev-parse --show-toplevel`. Labels stay relative to `root`.
#[cqrsy::handler(query)]
pub fn execute(
    req: FindRepoTops,
    discovery: &impl RepoDiscovery,
    git: &impl GitRunner,
) -> Result<Vec<DiscoveredRepo>, FindRepoTopsError> {
    let FindRepoTops {
        root,
        include_worktrees,
    } = req;
    let discovered = find_repos::execute(
        find_repos::DiscoverRepos {
            root,
            include_worktrees,
        },
        discovery,
    )
    .map_err(anyhow::Error::from)?;
    discovered
        .into_iter()
        .map(|repo| {
            Ok(DiscoveredRepo {
                path: top_level(git, &repo.path)?,
                label: repo.label,
            })
        })
        .collect()
}

/// The repo's canonical top-level path, or the legacy-shaped error (git's own
/// diagnostic, then `not a git repo: <path>`) when `dir` is not a git repo.
fn top_level(git: &impl GitRunner, dir: &Path) -> Result<PathBuf, FindRepoTopsError> {
    let out = git
        .run(dir, &["rev-parse", "--show-toplevel"])
        .map_err(anyhow::Error::from)?;
    if out.success() {
        return Ok(PathBuf::from(out.stdout.trim()));
    }
    let said = out.diagnostic();
    let error = if said.is_empty() {
        anyhow::anyhow!("not a git repo: {}", dir.display())
    } else {
        anyhow::anyhow!("{said}\nnot a git repo: {}", dir.display())
    };
    Err(FindRepoTopsError::Unexpected(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeGitRunner, FakeRepoDiscovery};

    #[test]
    fn resolves_each_discovered_repo_to_its_top_level() {
        let discovery = FakeRepoDiscovery {
            repos: vec!["/work/api".into(), "/work/libs/inner".into()],
        };
        let runner = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/real/api\n"),
            FakeGitRunner::ok("/real/libs/inner\n"),
        ]);

        let tops = execute(
            FindRepoTops {
                root: "/work".into(),
                include_worktrees: false,
            },
            &discovery,
            &runner,
        )
        .expect("discovery succeeds");

        assert_eq!(
            tops,
            vec![
                DiscoveredRepo {
                    path: "/real/api".into(),
                    label: "api".into(),
                },
                DiscoveredRepo {
                    path: "/real/libs/inner".into(),
                    label: "libs/inner".into(),
                },
            ]
        );
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["rev-parse", "--show-toplevel"],
                vec!["rev-parse", "--show-toplevel"],
            ]
        );
    }

    #[test]
    fn a_failed_top_level_resolution_keeps_the_legacy_error_shape() {
        let discovery = FakeRepoDiscovery {
            repos: vec!["/work/api".into()],
        };
        let runner = FakeGitRunner::new(vec![FakeGitRunner::exit_err("fatal: not a repo", 128)]);

        let error = execute(
            FindRepoTops {
                root: "/work".into(),
                include_worktrees: false,
            },
            &discovery,
            &runner,
        )
        .expect_err("resolution fails");

        assert_eq!(
            error.to_string(),
            "fatal: not a repo\nnot a git repo: /work/api"
        );
    }
}
