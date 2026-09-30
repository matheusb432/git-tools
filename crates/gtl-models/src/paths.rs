//! Validated filesystem and project identity roles.

use std::{
    ffi::OsStr,
    fmt,
    path::{Component, Path, PathBuf},
};

use nutype::nutype;
use serde::{Deserialize, Deserializer, Serialize};

const PROJECT_NAME_FALLBACK: &str = "repo";

/// An absolute root established by filesystem or Git resolution.
/// Windows verbatim prefixes are simplified when the ordinary spelling is safe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    /// Constructs a root from an absolute path returned by Git or filesystem resolution.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryRootError`] when `path` is not absolute.
    pub fn try_new(path: PathBuf) -> Result<Self, RepositoryRootError> {
        if is_model_absolute_path(&path) {
            let simplified = dunce::simplified(&path);
            if simplified == path {
                Ok(Self(path))
            } else {
                Ok(Self(simplified.to_path_buf()))
            }
        } else {
            Err(RepositoryRootError)
        }
    }

    /// Derives the project name from the final UTF-8 path component.
    pub fn project_name(&self) -> ProjectName {
        self.0
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(|name| ProjectName::try_new(name.to_owned()).ok())
            .unwrap_or_default()
    }

    /// Resolves a validated repository-relative path beneath this root.
    #[must_use]
    pub fn join(&self, path: &RepositoryRelativePath) -> AbsoluteFilePath {
        known_valid(AbsoluteFilePath::try_new(self.0.join(path.as_ref())))
    }
}

impl AsRef<Path> for RepositoryRoot {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for RepositoryRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl fmt::Display for RepositoryRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(formatter)
    }
}

impl TryFrom<PathBuf> for RepositoryRoot {
    type Error = RepositoryRootError;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RepositoryRoot> for PathBuf {
    fn from(value: RepositoryRoot) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for RepositoryRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_new(PathBuf::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Reports a non-absolute repository root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("repository root must be absolute")]
pub struct RepositoryRootError;

/// A non-empty project identifier.
#[nutype(
    validate(with = validate_project_name, error = ProjectNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        TryFrom,
        Into,
        Display,
        Default,
        Serialize,
        Deserialize
    ),
    default = PROJECT_NAME_FALLBACK.to_owned()
)]
pub struct ProjectName(String);

fn validate_project_name(value: &str) -> Result<(), ProjectNameError> {
    if value.is_empty() {
        Err(ProjectNameError)
    } else {
        Ok(())
    }
}

/// Reports an empty project name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("project name must not be empty")]
pub struct ProjectNameError;

/// A normalized path addressed from a repository root.
#[nutype(
    validate(with = validate_repository_relative_path, error = RepositoryRelativePathError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        TryFrom,
        Into,
        Serialize,
        Deserialize
    )
)]
pub struct RepositoryRelativePath(PathBuf);

/// Reports why a path cannot identify a file within a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RepositoryRelativePathError {
    #[error("repository-relative path must not be empty")]
    Empty,
    #[error("repository-relative path must not be absolute")]
    Absolute,
    #[error("repository-relative path must not traverse to a parent")]
    ParentTraversal,
    #[error("repository-relative path must be normalized")]
    NotNormalized,
}

/// An absolute filesystem path to a file or artifact.
#[nutype(
    validate(with = validate_absolute_file_path, error = AbsoluteFilePathError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        TryFrom,
        Into,
        Serialize,
        Deserialize
    )
)]
pub struct AbsoluteFilePath(PathBuf);

/// Reports a non-absolute file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("absolute file path must be absolute")]
pub struct AbsoluteFilePathError;

fn validate_absolute_file_path(path: &Path) -> Result<(), AbsoluteFilePathError> {
    if is_model_absolute_path(path) {
        Ok(())
    } else {
        Err(AbsoluteFilePathError)
    }
}

fn is_model_absolute_path(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    #[cfg(target_arch = "wasm32")]
    {
        path.to_str().is_some_and(is_portable_absolute_path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn is_portable_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with(r"\\")
        || bytes.first().is_some_and(u8::is_ascii_alphabetic)
            && bytes.get(1) == Some(&b':')
            && bytes
                .get(2)
                .is_some_and(|separator| matches!(separator, b'/' | b'\\'))
}

#[allow(clippy::unreachable)]
fn known_valid<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => unreachable!("a value assembled from validated parts remains valid"),
    }
}

fn validate_repository_relative_path(path: &Path) -> Result<(), RepositoryRelativePathError> {
    if path.as_os_str().is_empty() {
        return Err(RepositoryRelativePathError::Empty);
    }
    if path.is_absolute() {
        return Err(RepositoryRelativePathError::Absolute);
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(RepositoryRelativePathError::ParentTraversal);
            }
            Component::CurDir => return Err(RepositoryRelativePathError::NotNormalized),
            Component::Prefix(_) | Component::RootDir => {
                return Err(RepositoryRelativePathError::Absolute);
            }
        }
    }
    if let Some(path) = path.to_str() {
        validate_portable_repository_relative_path(path)?;
    }
    Ok(())
}

fn validate_portable_repository_relative_path(
    path: &str,
) -> Result<(), RepositoryRelativePathError> {
    let bytes = path.as_bytes();
    let has_windows_root = path.starts_with(['/', '\\'])
        || bytes.get(1) == Some(&b':')
            && bytes
                .get(2)
                .is_some_and(|separator| matches!(separator, b'/' | b'\\'));
    if has_windows_root {
        return Err(RepositoryRelativePathError::Absolute);
    }
    for component in path.split(['/', '\\']) {
        match component {
            ".." => return Err(RepositoryRelativePathError::ParentTraversal),
            "" | "." => return Err(RepositoryRelativePathError::NotNormalized),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn absolute_roles_reject_relative_paths() {
        assert_eq!(
            RepositoryRoot::try_new(PathBuf::from("repo")),
            Err(RepositoryRootError)
        );
        assert_eq!(
            AbsoluteFilePath::try_new(PathBuf::from("artifact.html")),
            Err(AbsoluteFilePathError)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_resolved_roots_keep_the_same_identity_as_directory_sources() {
        use crate::projects::catalogue::ProjectDirectorySource;

        let ordinary = RepositoryRoot::try_new(PathBuf::from(r"C:\repos\project")).unwrap();
        let resolved = RepositoryRoot::try_new(PathBuf::from(r"\\?\C:\repos\project")).unwrap();
        let source = ProjectDirectorySource::try_new(r"\\?\C:\repos\project".to_owned()).unwrap();

        assert_eq!(resolved, ordinary);
        assert_eq!(source.resolve().unwrap(), ordinary);
        assert_eq!(source.as_ref(), ordinary.to_string());
        assert_eq!(
            serde_json::to_value(&resolved).unwrap(),
            r"C:\repos\project"
        );
        assert_eq!(
            serde_json::from_str::<RepositoryRoot>(r#""\\\\?\\C:\\repos\\project""#).unwrap(),
            ordinary
        );
        let reserved = RepositoryRoot::try_new(PathBuf::from(r"\\?\C:\repos\COM1")).unwrap();
        assert_eq!(reserved.as_ref(), Path::new(r"\\?\C:\repos\COM1"));
    }

    #[test]
    fn portable_absolute_paths_cover_unix_windows_and_unc_syntax() {
        for path in [
            "/repos/git-tools",
            r"C:\repos\git-tools",
            "D:/repos/git-tools",
            r"\\server\share\git-tools",
        ] {
            assert!(is_portable_absolute_path(path), "{path}");
        }
        for path in ["repos/git-tools", r"repos\git-tools", r"C:git-tools"] {
            assert!(!is_portable_absolute_path(path), "{path}");
        }
    }

    #[test]
    fn repository_relative_paths_reject_absolute_and_traversing_values() {
        for path in [
            "/etc/passwd",
            "../secret",
            "src/../../secret",
            r"..\secret",
            r"src\..\secret",
            r"C:\secret",
        ] {
            assert!(
                RepositoryRelativePath::try_new(PathBuf::from(path)).is_err(),
                "{path} must be rejected"
            );
        }
    }

    #[test]
    fn repository_relative_paths_require_normalized_file_addresses() {
        for path in ["", ".", "./src/lib.rs", "src//lib.rs", r"src\\lib.rs"] {
            assert!(
                RepositoryRelativePath::try_new(PathBuf::from(path)).is_err(),
                "{path} must be rejected"
            );
        }
        assert_eq!(
            RepositoryRelativePath::try_new(PathBuf::from("src/lib.rs"))
                .unwrap()
                .as_ref(),
            Path::new("src/lib.rs")
        );
    }

    #[test]
    fn repository_root_joins_validated_relative_paths_into_absolute_files() {
        let root =
            RepositoryRoot::try_new(PathBuf::from("//fixture.invalid/repositories/repos/gt"))
                .unwrap();
        let relative = RepositoryRelativePath::try_new(PathBuf::from("src/lib.rs")).unwrap();

        assert_eq!(
            root.join(&relative).as_ref(),
            Path::new("//fixture.invalid/repositories/repos/gt/src/lib.rs")
        );
    }

    #[test]
    fn serde_preserves_primitive_wire_shapes_and_validates_decoding() {
        let root =
            RepositoryRoot::try_new(PathBuf::from("//fixture.invalid/repositories/repos/gt"))
                .unwrap();
        let project = ProjectName::try_new("git-tools".to_owned()).unwrap();
        let relative = RepositoryRelativePath::try_new(PathBuf::from("src/lib.rs")).unwrap();
        let absolute = root.join(&relative);

        assert_eq!(
            serde_json::to_value(&root).unwrap(),
            "//fixture.invalid/repositories/repos/gt"
        );
        assert_eq!(serde_json::to_value(&project).unwrap(), "git-tools");
        assert_eq!(serde_json::to_value(&relative).unwrap(), "src/lib.rs");
        assert_eq!(
            serde_json::to_value(&absolute).unwrap(),
            root.as_ref()
                .join("src/lib.rs")
                .to_string_lossy()
                .into_owned()
        );
        assert!(serde_json::from_str::<RepositoryRoot>(r#""relative""#).is_err());
        assert!(serde_json::from_str::<RepositoryRelativePath>(r#""../secret""#).is_err());
        assert!(serde_json::from_str::<AbsoluteFilePath>(r#""relative""#).is_err());
        assert!(serde_json::from_str::<ProjectName>(r#""""#).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn path_roles_preserve_non_utf8_filesystem_components() {
        use std::os::unix::ffi::OsStringExt as _;

        let mut root_path = PathBuf::from("//fixture.invalid/repositories/repos");
        root_path.push(std::ffi::OsString::from_vec(vec![b'g', 0x80, b't']));
        let root = RepositoryRoot::try_new(root_path.clone()).unwrap();
        let relative =
            RepositoryRelativePath::try_new(PathBuf::from(std::ffi::OsString::from_vec(vec![
                b'f', 0x80,
            ])))
            .unwrap();

        assert_eq!(root.as_ref(), root_path);
        assert_eq!(root.project_name().as_ref(), PROJECT_NAME_FALLBACK);
        assert_eq!(
            root.join(&relative).as_ref(),
            root.as_ref().join(relative.as_ref()).as_path()
        );
    }
}
