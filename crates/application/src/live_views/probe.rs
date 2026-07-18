//! The `live_views/probe` vertical slice: classify a live-view source's
//! directory (found / missing / not a repo) through the same [`RepoProbe`]
//! port `save` validates against, without persisting anything. This gives the
//! app-render path (`open_recipe`/restoring a live view) the same typed
//! `DirNotFound`/`DirNotGitRepo` rejection the daemon's `save_live_view`
//! already surfaces.

use std::path::PathBuf;

use domain::live_views::LiveSource;

use crate::{
    live_views::save::LiveViewRejection,
    ports::{RepoProbe, RepoProbeResult},
};

/// Probe the directory identified by `source_kind`/`source_value` for git-repo
/// validity. `data_root` is carried for parity with the other live-view
/// operations even though probing itself never touches the app-state store.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSource {
    pub data_root: PathBuf,
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSourceResponse {
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
    #[error("unknown live-view source kind `{kind}`")]
    UnknownSourceKind { kind: String },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Probes a source by rebuilding its [`LiveSource`] identity and
/// probing its directory through the [`RepoProbe`] port.
#[cqrsy::query]
pub fn execute(
    req: ProbeSource,
    probe: &impl RepoProbe,
) -> Result<ProbeSourceResponse, ProbeSourceError> {
    let ProbeSource {
        source_kind,
        source_value,
        ..
    } = req;
    let source = LiveSource::from_parts(&source_kind, &source_value)
        .ok_or_else(|| ProbeSourceError::UnknownSourceKind { kind: source_kind })?;
    let LiveSource::LocalRepo { path } = source;

    let outcome = match probe.probe(&path)? {
        RepoProbeResult::Repo { .. } => ProbeOutcome::Ok,
        RepoProbeResult::NotFound => ProbeOutcome::Broken {
            rejection: LiveViewRejection::DirNotFound {
                path: path.display().to_string(),
            },
        },
        RepoProbeResult::NotAGitRepo => ProbeOutcome::Broken {
            rejection: LiveViewRejection::DirNotGitRepo {
                path: path.display().to_string(),
            },
        },
    };

    Ok(ProbeSourceResponse { outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::RepoProbeResult, testing::FakeRepoProbe};

    fn probe(probe_result: RepoProbeResult) -> FakeRepoProbe {
        FakeRepoProbe {
            result: probe_result,
        }
    }

    fn request(source_value: &str) -> ProbeSource {
        ProbeSource {
            data_root: "/data".into(),
            source_kind: "LocalRepo".into(),
            source_value: source_value.into(),
        }
    }

    #[test]
    fn missing_dir_reports_broken_with_dir_not_found() {
        let probe = probe(RepoProbeResult::NotFound);
        let response = execute(request("/gone"), &probe).expect("probe succeeds");

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
        let probe = probe(RepoProbeResult::NotAGitRepo);
        let response = execute(request("/plain"), &probe).expect("probe succeeds");

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
        let probe = probe(RepoProbeResult::Repo {
            top_level: "/repos/gt".into(),
        });
        let response = execute(request("/repos/gt"), &probe).expect("probe succeeds");

        assert_eq!(response.outcome, ProbeOutcome::Ok);
    }

    #[test]
    fn unknown_source_kind_errors() {
        let probe = probe(RepoProbeResult::NotFound);
        let err = execute(
            ProbeSource {
                data_root: "/data".into(),
                source_kind: "GithubRepo".into(),
                source_value: "owner/repo".into(),
            },
            &probe,
        )
        .expect_err("unknown kind rejects");

        assert!(
            matches!(err, ProbeSourceError::UnknownSourceKind { kind } if kind == "GithubRepo")
        );
    }
}
