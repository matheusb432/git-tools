//! Selects present managed repositories whose HEAD is ahead of its upstream.

use domain::{discovery::DiscoveredRepo, managed::ManagedRepo};

use crate::ports::GitClient;

/// Requests unpushed selection across already-resolved managed repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectUnpushed {
    pub repos: Vec<ManagedRepo>,
}

pub type SelectUnpushedOk = Vec<DiscoveredRepo>;

/// Reports an unexpected Git failure while selecting unpushed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SelectUnpushedError {
    /// Git transport failed while resolving a repository's upstream.
    #[error("{source}")]
    Upstream {
        repo: String,
        #[source]
        source: anyhow::Error,
    },
    /// Git failed while counting commits ahead of a repository's upstream.
    #[error("{source}")]
    Count {
        repo: String,
        #[source]
        source: anyhow::Error,
    },
    /// Git failed while resolving a selected repository's top-level path.
    #[error("{source}")]
    TopLevel {
        repo: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Returns every present managed repository with commits ahead of its upstream.
///
/// Repositories without an upstream, with an invalid ahead count, or with zero
/// commits ahead are expected omissions rather than errors.
///
/// # Errors
///
/// Returns [`SelectUnpushedError`] when Git transport fails, the ahead-count
/// command is rejected, or an ahead repository's top-level path cannot be resolved.
#[cqrsy::query]
pub fn execute(
    query: SelectUnpushed,
    git: &impl GitClient,
) -> Result<SelectUnpushedOk, SelectUnpushedError> {
    let mut selected = Vec::new();
    for repo in query.repos {
        if !git.repo_present(&repo.path) {
            continue;
        }

        match git
            .upstream(&repo.path)
            .map_err(|source| SelectUnpushedError::Upstream {
                repo: repo.name.clone(),
                source,
            })? {
            crate::ports::GitEffect::Applied(_) => {}
            crate::ports::GitEffect::Rejected(_) => continue,
        }

        let count = git
            .commit_count(&repo.path, "@{u}..HEAD")
            .map_err(|source| SelectUnpushedError::Count {
                repo: repo.name.clone(),
                source,
            })?;
        let Some(count) = count else { continue };
        if count == 0 {
            continue;
        }

        let top = git
            .discover_top(&repo.path)
            .map_err(|source| SelectUnpushedError::TopLevel {
                repo: repo.name.clone(),
                source,
            })?;
        let Some(top) = top else {
            return Err(SelectUnpushedError::TopLevel {
                repo: repo.name,
                source: anyhow::anyhow!("not a git repository"),
            });
        };
        selected.push(DiscoveredRepo {
            path: top,
            label: repo.name,
        });
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use domain::{discovery::DiscoveredRepo, managed::ManagedRepo};

    use super::{SelectUnpushed, SelectUnpushedError, execute};
    use crate::testing::ScriptedGitClient;

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: String::new(),
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

        let selected = execute(
            SelectUnpushed {
                repos: vec![
                    repo("absent"),
                    repo("api"),
                    repo("clean"),
                    repo("untracked"),
                    repo("invalid"),
                ],
            },
            &git,
        )
        .expect("Git transport remains available");

        assert_eq!(
            selected,
            vec![DiscoveredRepo {
                path: "/repos/api".into(),
                label: "api".into(),
            }]
        );
    }

    #[test]
    fn transport_failure_preserves_the_source_and_repository() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            SelectUnpushed {
                repos: vec![repo("api")],
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let SelectUnpushedError::Upstream { repo, source } = error else {
            panic!("the upstream transport stage must remain identifiable");
        };
        assert_eq!(repo, "api");
        assert_eq!(source.to_string(), "git transport unavailable");
    }
}
