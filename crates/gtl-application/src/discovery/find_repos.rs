//! The `discovery/find_repos` slice: walk `root` through the
//! [`RepoDiscovery`](crate::ports::RepoDiscovery) port and label each git repo
//! relative to it.

use std::path::{Path, PathBuf};

use gtl_models::discovery::DiscoveredRepo;

use crate::ports::RepoDiscovery;

/// Discover every git repo under `root`, labeled relative to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverRepos {
    pub root: PathBuf,
    pub include_worktrees: bool,
}

/// Everything that can go wrong discovering repos.
#[derive(Debug, thiserror::Error)]
pub enum DiscoverReposError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Walk `root` through the port and label each discovered repo relative to it.
#[cqrsy::query]
pub fn execute(
    req: DiscoverRepos,
    discovery: &impl RepoDiscovery,
) -> Result<Vec<DiscoveredRepo>, DiscoverReposError> {
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

fn repo_label(root: &Path, repo_path: &Path) -> String {
    let relative = repo_path.strip_prefix(root).unwrap_or(repo_path);
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        crate::shared::repository_name::from_path(repo_path)
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeRepoDiscovery;

    #[test]
    fn labels_each_discovered_repo_relative_to_the_root() {
        let discovery = FakeRepoDiscovery {
            repos: vec![
                "/work/api".into(),
                "/work/libs/inner".into(),
                "/work".into(),
            ],
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
                DiscoveredRepo {
                    path: "/work".into(),
                    label: "work".into(),
                },
            ]
        );
    }
}
