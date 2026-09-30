//! Selects project repositories with commits ahead of their effective comparison.

use gtl_models::{
    git::{CommitCount, GitRange},
    paths::ProjectName,
    projects::ProjectRepository,
    repository::traversal::RepositoryTarget,
};

use crate::ports::GitClient;

#[derive(Debug)]
pub struct SelectedProjectRepositories {
    pub repositories: Vec<RepositoryTarget>,
    pub notes: Vec<crate::shared::notes::Note>,
}

/// Reports an unexpected Git failure while selecting project comparisons.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SelectComparisonRepositoriesError {
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

/// Selects nonempty upstream or local-branch comparisons and reports unavailable bases.
///
/// Missing repositories and comparisons with no commits ahead are omitted.
/// Missing local bases and unrelated histories are omitted with named warnings.
///
/// # Errors
///
/// Returns [`SelectComparisonRepositoriesError`] when Git transport fails, the ahead-count
/// command is rejected, or an ahead repository's top-level path cannot be resolved.
#[cqrsy::query]
pub fn execute(
    repos: Vec<ProjectRepository>,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<SelectedProjectRepositories, SelectComparisonRepositoriesError> {
    let mut selected = Vec::new();
    let mut notes = Vec::new();
    for repo in repos {
        if !git.repo_present(&repo.path) {
            continue;
        }

        let range = match git.upstream(&repo.path).map_err(|source| {
            SelectComparisonRepositoriesError::Upstream {
                repo: repo.name.clone(),
                source,
            }
        })? {
            crate::ports::GitEffect::Applied(reference) => GitRange::two_dot(
                &gtl_models::git::GitRevision::from(&reference),
                &gtl_models::git::GitRevision::head(),
            ),
            crate::ports::GitEffect::Rejected(_) => {
                match super::comparison::resolve_local(&repo.path, git, comparisons) {
                    Ok(comparison) => comparison.commit_range(),
                    Err(error) if error.is_unavailable() => {
                        notes.push(crate::shared::notes::Note::warn(format!(
                            "{}: {error}",
                            repo.name
                        )));
                        continue;
                    }
                    Err(error) => {
                        return Err(SelectComparisonRepositoriesError::Upstream {
                            repo: repo.name,
                            source: error.into(),
                        });
                    }
                }
            }
        };

        let count = git.commit_count(&repo.path, &range).map_err(|source| {
            SelectComparisonRepositoriesError::Count {
                repo: repo.name.clone(),
                source,
            }
        })?;
        let Some(count) = count else { continue };
        if count == CommitCount::default() {
            continue;
        }

        let top = git.discover_top(&repo.path).map_err(|source| {
            SelectComparisonRepositoriesError::TopLevel {
                repo: repo.name.clone(),
                source,
            }
        })?;
        let Some(top) = top else {
            return Err(SelectComparisonRepositoriesError::TopLevel {
                repo: repo.name,
                source: anyhow::anyhow!("not a git repository"),
            });
        };
        selected.push(RepositoryTarget {
            path: top,
            label: repo.name,
        });
    }
    Ok(SelectedProjectRepositories {
        repositories: selected,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use gtl_models::{projects::ProjectRepository, repository::traversal::RepositoryTarget};

    use super::SelectComparisonRepositoriesError;
    use crate::{projects::select_comparison_repositories, utils::ScriptedGitClient};

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!(
                "//fixture.invalid/repositories/repos/{name}"
            )),
            remote: None,
        }
    }

    #[test]
    fn select_unpushed_keeps_only_present_ahead_repositories() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/repos/api\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::rejected("no upstream"),
            ScriptedGitClient::rejected("missing comparison branch"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("invalid\n"),
        ]);
        git.absent_repos
            .lock()
            .unwrap()
            .push(PathBuf::from("//fixture.invalid/repositories/repos/absent"));

        let selected = select_comparison_repositories::execute(
            vec![
                repo("absent"),
                repo("api"),
                repo("clean"),
                repo("untracked"),
                repo("invalid"),
            ],
            &git,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(
            selected.repositories,
            vec![RepositoryTarget {
                path: crate::utils::repository_root("//fixture.invalid/repositories/repos/api"),
                label: crate::utils::project_name("api"),
            }]
        );
    }

    #[test]
    fn transport_failure_preserves_the_source_and_repository() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = select_comparison_repositories::execute(
            vec![repo("api")],
            &git,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap_err();

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let (repo, source) = match error {
            SelectComparisonRepositoriesError::Upstream { repo, source } => Some((repo, source)),
            SelectComparisonRepositoriesError::Count { .. }
            | SelectComparisonRepositoriesError::TopLevel { .. } => None,
        }
        .unwrap();
        assert_eq!(repo.as_str(), "api");
        assert_eq!(source.to_string(), "git transport unavailable");
    }
}
