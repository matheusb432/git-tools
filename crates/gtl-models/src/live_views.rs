//! Live-view sources: the adapter-shaped identity (`kind` + `value`) of a
//! repository a saved live view renders from. Only `LocalRepo` exists today;
//! the enum leaves room for network-backed kinds without reshaping consumers.
//! Parsing stored parts is fallible by design — a persisted row with an
//! unknown kind must surface as a typed broken state, never a panic.

use serde::{Deserialize, Serialize};

use crate::paths::{ProjectName, RepositoryRoot, RepositoryRootError};

/// The identity of a live-view source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind")]
pub enum LiveSource {
    /// A repository on the local filesystem, keyed by its canonical top-level path.
    #[serde(rename = "LocalRepo")]
    LocalRepo {
        #[serde(rename = "source_value")]
        path: RepositoryRoot,
    },
}

impl LiveSource {
    /// A local-repo source for `path` (callers pass the canonical repo top-level).
    pub fn local_repo(path: RepositoryRoot) -> Self {
        Self::LocalRepo { path }
    }

    /// The stable kind discriminant persisted as `source_kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::LocalRepo { .. } => "LocalRepo",
        }
    }

    /// The identity payload persisted as `source_value`.
    pub fn value(&self) -> String {
        match self {
            Self::LocalRepo { path } => path.display().to_string(),
        }
    }

    /// A human-facing default label for this source.
    pub fn display_name(&self) -> ProjectName {
        match self {
            Self::LocalRepo { path } => path.project_name(),
        }
    }

    /// Rebuilds a source from persisted scalar columns.
    pub fn from_parts(kind: &str, value: &str) -> Result<Self, ParseLiveSourceError> {
        match kind {
            "LocalRepo" => RepositoryRoot::try_new(value.into())
                .map(Self::local_repo)
                .map_err(ParseLiveSourceError::InvalidRepositoryRoot),
            _ => Err(ParseLiveSourceError::UnknownKind {
                kind: kind.to_owned(),
            }),
        }
    }
}

/// Reports an unsupported persisted live-source kind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseLiveSourceError {
    #[error("unknown live-view source kind `{kind}`")]
    UnknownKind { kind: String },
    #[error("invalid live-view repository root: {0}")]
    InvalidRepositoryRoot(#[from] RepositoryRootError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &str) -> RepositoryRoot {
        RepositoryRoot::try_new(path.into()).expect("absolute repository root")
    }

    #[test]
    fn local_repo_identity_is_kind_plus_path() {
        let source = LiveSource::local_repo(root("/home/u/tools/gt"));
        assert_eq!(source.kind(), "LocalRepo");
        assert_eq!(source.value(), "/home/u/tools/gt");
    }

    #[test]
    fn display_name_is_the_last_path_component() {
        let source = LiveSource::local_repo(root("/home/u/tools/gt"));
        assert_eq!(source.display_name().as_str(), "gt");
    }

    #[test]
    fn from_parts_round_trips_local_repo() {
        let source = LiveSource::from_parts("LocalRepo", "/x/y").expect("known kind parses");
        assert_eq!(source, LiveSource::local_repo(root("/x/y")));
    }

    #[test]
    fn from_parts_rejects_unknown_kinds() {
        assert_eq!(
            LiveSource::from_parts("GithubRepo", "owner/repo").unwrap_err(),
            ParseLiveSourceError::UnknownKind {
                kind: "GithubRepo".into()
            }
        );
    }

    #[test]
    fn from_parts_rejects_non_absolute_repository_roots() {
        assert_eq!(
            LiveSource::from_parts("LocalRepo", "relative").unwrap_err(),
            ParseLiveSourceError::InvalidRepositoryRoot(RepositoryRootError)
        );
    }

    #[test]
    fn serde_preserves_the_flat_persistence_shape() {
        let source = LiveSource::local_repo(root("/x/y"));

        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "source_kind": "LocalRepo",
                "source_value": "/x/y"
            })
        );
        assert_eq!(serde_json::from_value::<LiveSource>(json).unwrap(), source);
    }
}
