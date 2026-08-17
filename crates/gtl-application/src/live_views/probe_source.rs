//! The `live_views/probe` vertical slice: classify a live-view source's
//! directory (found / missing / not a repo) through the same [`GitClient`]
//! capability `save` validates against, without persisting anything. This gives the
//! app-render path (`open_recipe`/restoring a live view) the same typed
//! `DirNotFound`/`DirNotGitRepo` rejection the daemon's `save_live_view`
//! already surfaces.

use gtl_models::live_views::LiveSource;

use crate::{
    live_views::save_live_view::LiveViewRejection,
    ports::{GitClient, GitRepositoryState},
};

/// Probes one validated live-view source for git-repository validity.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSource {
    pub source: LiveSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSourceOk {
    pub outcome: ProbeOutcome,
}

/// What probing a live-view source found.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    /// The source's directory validates as a git repo.
    Ok,
    /// The source's directory failed to validate; the typed rejection.
    Broken { rejection: LiveViewRejection },
}

/// Everything that can go wrong probing a live-view source.
#[derive(Debug, thiserror::Error)]
pub enum ProbeSourceError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Probes a source's directory through the [`GitClient`] capability.
#[cqrsy::query]
pub fn execute(req: ProbeSource, git: &impl GitClient) -> Result<ProbeSourceOk, ProbeSourceError> {
    let ProbeSource { source } = req;
    let LiveSource::LocalRepo { path } = source;

    let outcome = match git.probe_repository(&path)? {
        GitRepositoryState::Repository { .. } => ProbeOutcome::Ok,
        GitRepositoryState::NotFound => ProbeOutcome::Broken {
            rejection: LiveViewRejection::DirNotFound {
                path: path.display().to_string(),
            },
        },
        GitRepositoryState::NotARepository => ProbeOutcome::Broken {
            rejection: LiveViewRejection::DirNotGitRepo {
                path: path.display().to_string(),
            },
        },
    };

    Ok(ProbeSourceOk { outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{live_views::probe_source, ports::GitRepositoryState, utils::FakeGitClient};

    fn git(repository_state: GitRepositoryState) -> FakeGitClient {
        FakeGitClient {
            repository_state: Some(repository_state),
            ..Default::default()
        }
    }

    fn request(source_value: &str) -> ProbeSource {
        ProbeSource {
            source: LiveSource::local_repo(crate::utils::repository_root(source_value)),
        }
    }

    #[test]
    fn missing_dir_reports_broken_with_dir_not_found() {
        let git = git(GitRepositoryState::NotFound);
        let response = probe_source::execute(request("/gone"), &git).expect("probe succeeds");

        match response.outcome {
            ProbeOutcome::Broken { rejection } => {
                assert_eq!(rejection.code(), "DirNotFound");
                assert_eq!(
                    rejection.to_string(),
                    "The git repo's directory at `/gone` was not found."
                );
            }
            ProbeOutcome::Ok => panic!("expected Broken, got Ok"),
        }
    }

    #[test]
    fn non_repo_dir_reports_broken_with_dir_not_git_repo() {
        let git = git(GitRepositoryState::NotARepository);
        let response = probe_source::execute(request("/plain"), &git).expect("probe succeeds");

        match response.outcome {
            ProbeOutcome::Broken { rejection } => {
                assert_eq!(rejection.code(), "DirNotGitRepo");
                assert_eq!(
                    rejection.to_string(),
                    "The directory `/plain` is not a git repository."
                );
            }
            ProbeOutcome::Ok => panic!("expected Broken, got Ok"),
        }
    }

    #[test]
    fn valid_repo_reports_ok() {
        let git = git(GitRepositoryState::Repository {
            top_level: crate::utils::repository_root("/repos/gt"),
        });
        let response = probe_source::execute(request("/repos/gt"), &git).expect("probe succeeds");

        assert_eq!(response.outcome, ProbeOutcome::Ok);
    }
}
