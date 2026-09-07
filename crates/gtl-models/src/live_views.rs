use serde::{Deserialize, Serialize};

use crate::paths::{ProjectName, RepositoryRoot, RepositoryRootError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveComparison {
    LocalChanges,
    UnpushedCommits,
}

impl LiveComparison {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalChanges => "local_changes",
            Self::UnpushedCommits => "unpushed_commits",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LocalChanges => "Local changes",
            Self::UnpushedCommits => "Unpushed commits",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind")]
pub enum LiveSource {
    #[serde(rename = "LocalRepo")]
    LocalRepo {
        #[serde(rename = "source_value")]
        path: RepositoryRoot,
    },
}

impl LiveSource {
    #[must_use]
    pub fn local_repo(path: RepositoryRoot) -> Self {
        Self::LocalRepo { path }
    }

    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::LocalRepo { .. } => "LocalRepo",
        }
    }

    #[must_use]
    pub fn value(&self) -> String {
        match self {
            Self::LocalRepo { path } => path.display().to_string(),
        }
    }

    #[must_use]
    pub fn display_name(&self) -> ProjectName {
        match self {
            Self::LocalRepo { path } => path.project_name(),
        }
    }

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
        RepositoryRoot::try_new(path.into()).unwrap()
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
        let source = LiveSource::from_parts("LocalRepo", "/x/y").unwrap();
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
