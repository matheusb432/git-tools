//! Live-view sources: the adapter-shaped identity (`kind` + `value`) of a
//! repository a saved live view renders from. Only `LocalRepo` exists today;
//! the enum leaves room for network-backed kinds without reshaping consumers.
//! Parsing stored parts is fallible by design — a persisted row with an
//! unknown kind must surface as a typed broken state, never a panic.

use std::path::PathBuf;

/// The identity of a live-view source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveSource {
    /// A repository on the local filesystem, keyed by its canonical top-level path.
    LocalRepo { path: PathBuf },
}

impl LiveSource {
    /// A local-repo source for `path` (callers pass the canonical repo top-level).
    pub fn local_repo(path: PathBuf) -> Self {
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
    pub fn display_name(&self) -> String {
        match self {
            Self::LocalRepo { path } => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }

    /// Rebuild a source from persisted parts; `None` for an unknown kind.
    pub fn from_parts(kind: &str, value: &str) -> Option<Self> {
        match kind {
            "LocalRepo" => Some(Self::LocalRepo {
                path: PathBuf::from(value),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn local_repo_identity_is_kind_plus_path() {
        let source = LiveSource::local_repo(PathBuf::from("/home/u/tools/gt"));
        assert_eq!(source.kind(), "LocalRepo");
        assert_eq!(source.value(), "/home/u/tools/gt");
    }

    #[test]
    fn display_name_is_the_last_path_component() {
        let source = LiveSource::local_repo(PathBuf::from("/home/u/tools/gt"));
        assert_eq!(source.display_name(), "gt");
    }

    #[test]
    fn from_parts_round_trips_local_repo() {
        let source = LiveSource::from_parts("LocalRepo", "/x/y").expect("known kind parses");
        assert_eq!(source, LiveSource::local_repo(PathBuf::from("/x/y")));
    }

    #[test]
    fn from_parts_rejects_unknown_kinds() {
        assert_eq!(LiveSource::from_parts("GithubRepo", "owner/repo"), None);
        assert_eq!(LiveSource::from_parts("garbage", "x"), None);
    }
}
