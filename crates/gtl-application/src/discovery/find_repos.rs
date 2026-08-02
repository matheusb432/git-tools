//! The `discovery/find_repos` slice: walk `root` through the
//! [`RepoDiscovery`](crate::ports::RepoDiscovery) port and label each git repo
//! relative to it.

use std::path::PathBuf;

use gtl_models::discovery::DiscoveredRepo;

use crate::{discovery::rules::repo_label, ports::RepoDiscovery};

/// Discover every git repo under `root`, labeled relative to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverRepos {
    pub root: PathBuf,
    pub include_worktrees: bool,
}

pub type DiscoverReposOk = Vec<DiscoveredRepo>;

/// Everything that can go wrong discovering repos.
#[derive(Debug, thiserror::Error)]
pub enum DiscoverError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Walk `root` through the port and label each discovered repo relative to it.
#[cqrsy::query]
pub fn execute(
    req: DiscoverRepos,
    discovery: &impl RepoDiscovery,
) -> Result<DiscoverReposOk, DiscoverError> {
    let DiscoverRepos {
        root,
        include_worktrees,
    } = req;
    let repos = discovery.find_repos(&root, include_worktrees)?;
    Ok(repos
        .into_iter()
        .map(|repo| DiscoveredRepo {
            label: repo_label(&root, &repo),
            path: repo,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeRepoDiscovery;

    #[test]
    fn labels_each_discovered_repo_relative_to_the_root() {
        let discovery = FakeRepoDiscovery {
            repos: vec!["/work/api".into(), "/work/libs/inner".into()],
        };
        let out = execute(
            DiscoverRepos {
                root: "/work".into(),
                include_worktrees: false,
            },
            &discovery,
        )
        .expect("discovery succeeds");

        assert_eq!(
            out,
            vec![
                DiscoveredRepo {
                    path: "/work/api".into(),
                    label: "api".into(),
                },
                DiscoveredRepo {
                    path: "/work/libs/inner".into(),
                    label: "libs/inner".into(),
                },
            ]
        );
    }
}
