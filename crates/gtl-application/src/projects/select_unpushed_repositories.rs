//! Selects present project repositories whose HEAD is ahead of its upstream.

use gtl_models::{
    git::{CommitCount, GitRange},
    paths::ProjectName,
    projects::ProjectRepository,
    repository::traversal::RepositoryTarget,
};

use crate::ports::GitClient;

/// Reports an unexpected Git failure while selecting unpushed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SelectUnpushedRepositoriesError {
    /// Git transport failed while resolving a repository's upstream.
    #[error("{source}")]
    Upstream {
        repo: ProjectName,
        #[source]
        source: anyhow::Error,
    },
    /// Git failed while counting commits ahead of a repository's upstream.
    #[error("{source}")]
    Count {
        repo: ProjectName,
        #[source]
        source: anyhow::Error,
    },
    /// Git failed while resolving a selected repository's top-level path.
    #[error("{source}")]
    TopLevel {
        repo: ProjectName,
        #[source]
        source: anyhow::Error,
    },
}

/// Returns every present project repository with commits ahead of its upstream.
///
/// Repositories without an upstream, with an invalid ahead count, or with zero
/// commits ahead are expected omissions rather than errors.
///
/// # Errors
///
/// Returns [`SelectUnpushedRepositoriesError`] when Git transport fails, the ahead-count
/// command is rejected, or an ahead repository's top-level path cannot be resolved.
#[cqrsy::query]
pub fn execute(
    repos: Vec<ProjectRepository>,
    git: &impl GitClient,
) -> Result<Vec<RepositoryTarget>, SelectUnpushedRepositoriesError> {
    let mut selected = Vec::new();
    for repo in repos {
        if !git.repo_present(&repo.path) {
            continue;
        }

        match git.upstream(&repo.path).map_err(|source| {
            SelectUnpushedRepositoriesError::Upstream {
                repo: repo.name.clone(),
                source,
            }
        })? {
            crate::ports::GitEffect::Applied(_) => {}
            crate::ports::GitEffect::Rejected(_) => continue,
        }

        let count = git
            .commit_count(&repo.path, &GitRange::upstream_to_head())
            .map_err(|source| SelectUnpushedRepositoriesError::Count {
                repo: repo.name.clone(),
                source,
            })?;
        let Some(count) = count else { continue };
        if count == CommitCount::default() {
            continue;
        }

        let top = git.discover_top(&repo.path).map_err(|source| {
            SelectUnpushedRepositoriesError::TopLevel {
                repo: repo.name.clone(),
                source,
            }
        })?;
        let Some(top) = top else {
            return Err(SelectUnpushedRepositoriesError::TopLevel {
                repo: repo.name,
                source: anyhow::anyhow!("not a git repository"),
            });
        };
        selected.push(RepositoryTarget {
            path: top,
            label: repo.name,
        });
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use gtl_models::{projects::ProjectRepository, repository::traversal::RepositoryTarget};

    use super::SelectUnpushedRepositoriesError;
    use crate::{projects::select_unpushed_repositories, utils::ScriptedGitClient};

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!("/repos/{name}")),
            remote: None,
        }
    }

    #[test]
    fn select_unpushed_keeps_only_present_ahead_repositories() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::rejected("no upstream"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("invalid\n"),
        ]);
        git.absent_repos
            .lock()
            .unwrap()
            .push(PathBuf::from("/repos/absent"));

        let selected = select_unpushed_repositories::execute(
            vec![
                repo("absent"),
                repo("api"),
                repo("clean"),
                repo("untracked"),
                repo("invalid"),
            ],
            &git,
        )
        .unwrap();

        assert_eq!(
            selected,
            vec![RepositoryTarget {
                path: crate::utils::repository_root("/repos/api"),
                label: crate::utils::project_name("api"),
            }]
        );
    }

    #[test]
    fn transport_failure_preserves_the_source_and_repository() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = select_unpushed_repositories::execute(vec![repo("api")], &git).unwrap_err();

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let (repo, source) = match error {
            SelectUnpushedRepositoriesError::Upstream { repo, source } => Some((repo, source)),
            SelectUnpushedRepositoriesError::Count { .. }
            | SelectUnpushedRepositoriesError::TopLevel { .. } => None,
        }
        .unwrap();
        assert_eq!(repo.as_str(), "api");
        assert_eq!(source.to_string(), "git transport unavailable");
    }
}
