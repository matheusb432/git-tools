//! The `discovery/find_repo_tops` query: discover every git repo under a root
//! (via [`find_repos`]) and resolve each to its canonical top-level path through
//! the [`GitRunner`](crate::ports::GitRunner) port — the unit the `diff -r` wire
//! paths hand to the daemon.

use std::path::PathBuf;

use domain::discovery::DiscoveredRepo;

use crate::{
    discovery::{find_repos, resolve_repo_top},
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
#[non_exhaustive]
pub enum FindRepoTopsError {
    /// Reports that repositories could not be discovered under the requested root.
    #[error(transparent)]
    Discover(#[from] find_repos::DiscoverError),
    /// Reports that a discovered repository's canonical top level could not be resolved.
    #[error(transparent)]
    Resolve(#[from] resolve_repo_top::ResolveRepoTopError),
}

/// Walk `root` through the discovery slice and resolve each repo's `path` to its
/// `git rev-parse --show-toplevel`. Labels stay relative to `root`.
///
/// # Errors
///
/// Returns [`FindRepoTopsError::Discover`] when repository discovery fails, or
/// [`FindRepoTopsError::Resolve`] when a discovered repository's canonical top
/// level cannot be resolved.
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
    )?;
    discovered
        .into_iter()
        .map(|repo| {
            Ok(DiscoveredRepo {
                path: resolve_repo_top::execute(
                    resolve_repo_top::ResolveRepoTop { repo: repo.path },
                    git,
                )?,
                label: repo.label,
            })
        })
        .collect()
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
