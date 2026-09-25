//! Repository extension filters that hide changed files from diffs.

use std::{fmt, path::Path, str::FromStr};

use crate::paths::RepositoryRelativePath;

/// ASCII-lowercase, dotless, sorted, unique extensions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(from = "Vec<String>", into = "Vec<String>")]
pub struct FileExtensions(Vec<String>);

impl From<Vec<String>> for FileExtensions {
    fn from(raw: Vec<String>) -> Self {
        Self::new(raw)
    }
}

impl From<FileExtensions> for Vec<String> {
    fn from(value: FileExtensions) -> Self {
        value.0
    }
}

impl FileExtensions {
    pub fn new<I, S>(raw: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut extensions: Vec<String> = raw
            .into_iter()
            .map(|ext| {
                ext.as_ref()
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|ext| !ext.is_empty())
            .collect();
        extensions.sort();
        extensions.dedup();
        Self(extensions)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Reports whether `path`'s final extension is listed, ignoring ASCII case the way Git's
    /// `icase` pathspec magic does.
    #[must_use]
    pub fn contains_extension_of(&self, path: &RepositoryRelativePath) -> bool {
        if self.0.is_empty() {
            return false;
        }
        Path::new(path.as_path())
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                let ext = ext.to_ascii_lowercase();
                self.0.binary_search(&ext).is_ok()
            })
    }

    #[must_use]
    pub fn extensions(&self) -> &[String] {
        &self.0
    }

    fn intersection(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|extension| other.0.binary_search(extension).is_ok())
                .cloned()
                .collect(),
        )
    }

    fn difference(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|extension| other.0.binary_search(extension).is_err())
                .cloned()
                .collect(),
        )
    }

    fn union(&self, other: &Self) -> Self {
        let mut extensions = [self.0.as_slice(), other.0.as_slice()].concat();
        extensions.sort();
        extensions.dedup();
        Self(extensions)
    }
}

/// Chooses paths by final extension, so Git can select them with one pathspec per extension
/// instead of one per path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionSelection {
    /// Paths whose final extension is listed.
    Listed(FileExtensions),
    /// Paths whose final extension is not listed, including extensionless paths.
    Unlisted(FileExtensions),
}

impl ExtensionSelection {
    /// Selects every path.
    #[must_use]
    pub fn all() -> Self {
        Self::Unlisted(FileExtensions::default())
    }

    #[must_use]
    pub fn contains(&self, path: &RepositoryRelativePath) -> bool {
        match self {
            Self::Listed(extensions) => extensions.contains_extension_of(path),
            Self::Unlisted(extensions) => !extensions.contains_extension_of(path),
        }
    }

    /// Returns the listed or unlisted extensions.
    #[must_use]
    pub fn extensions(&self) -> &FileExtensions {
        match self {
            Self::Listed(extensions) | Self::Unlisted(extensions) => extensions,
        }
    }

    /// Selects the paths this selection leaves out.
    #[must_use]
    pub fn complement(self) -> Self {
        match self {
            Self::Listed(extensions) => Self::Unlisted(extensions),
            Self::Unlisted(extensions) => Self::Listed(extensions),
        }
    }

    /// Selects the paths chosen by both selections.
    ///
    /// Every path has at most one final extension, so the result stays a single list.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Listed(left), Self::Listed(right)) => Self::Listed(left.intersection(right)),
            (Self::Listed(listed), Self::Unlisted(unlisted))
            | (Self::Unlisted(unlisted), Self::Listed(listed)) => {
                Self::Listed(listed.difference(unlisted))
            }
            (Self::Unlisted(left), Self::Unlisted(right)) => Self::Unlisted(left.union(right)),
        }
    }
}

/// Chooses whether a filter hides its listed extensions or every other file.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    strum::VariantArray,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionFilterMode {
    /// Hides files whose extension is listed.
    #[default]
    Hide,
    /// Hides files whose extension is not listed, including extensionless files.
    Only,
}

impl ExtensionFilterMode {
    /// Names the persisted token for one mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Only => "only",
        }
    }
}

impl fmt::Display for ExtensionFilterMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown extension filter mode `{0}`")]
pub struct ParseExtensionFilterModeError(String);

impl FromStr for ExtensionFilterMode {
    type Err = ParseExtensionFilterModeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <Self as strum::VariantArray>::VARIANTS
            .iter()
            .copied()
            .find(|mode| mode.as_str() == value)
            .ok_or_else(|| ParseExtensionFilterModeError(value.to_owned()))
    }
}

/// Hides changed files by their final extension; an empty extension set hides nothing in either
/// mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExtensionFilter {
    mode: ExtensionFilterMode,
    extensions: FileExtensions,
}

impl ExtensionFilter {
    #[must_use]
    pub const fn new(mode: ExtensionFilterMode, extensions: FileExtensions) -> Self {
        Self { mode, extensions }
    }

    #[must_use]
    pub const fn mode(&self) -> ExtensionFilterMode {
        self.mode
    }

    #[must_use]
    pub const fn extensions(&self) -> &FileExtensions {
        &self.extensions
    }

    #[must_use]
    pub fn into_parts(self) -> (ExtensionFilterMode, FileExtensions) {
        (self.mode, self.extensions)
    }

    /// Reports whether the filter can hide any file.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.extensions.is_empty()
    }

    /// Selects the paths this filter shows.
    #[must_use]
    pub fn shown(&self) -> ExtensionSelection {
        match self.mode {
            ExtensionFilterMode::Hide => ExtensionSelection::Unlisted(self.extensions.clone()),
            ExtensionFilterMode::Only if self.is_active() => {
                ExtensionSelection::Listed(self.extensions.clone())
            }
            ExtensionFilterMode::Only => ExtensionSelection::all(),
        }
    }

    /// Selects the paths this filter hides.
    #[must_use]
    pub fn hidden(&self) -> ExtensionSelection {
        self.shown().complement()
    }

    #[must_use]
    pub fn hides(&self, path: &RepositoryRelativePath) -> bool {
        if !self.is_active() {
            return false;
        }
        let listed = self.extensions.contains_extension_of(path);
        match self.mode {
            ExtensionFilterMode::Hide => listed,
            ExtensionFilterMode::Only => !listed,
        }
    }
}

/// Records the files an active filter removed from one diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedExtensionFilter {
    pub filter: ExtensionFilter,
    pub hidden_paths: Vec<RepositoryRelativePath>,
}

impl AppliedExtensionFilter {
    #[must_use]
    pub fn from_hidden(
        filter: &ExtensionFilter,
        hidden_paths: Vec<RepositoryRelativePath>,
    ) -> Option<Self> {
        if hidden_paths.is_empty() {
            return None;
        }
        Some(Self {
            filter: filter.clone(),
            hidden_paths,
        })
    }

    #[must_use]
    pub fn extensions_label(&self) -> String {
        self.filter.extensions().extensions().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(value.into()).unwrap()
    }

    fn filter(mode: ExtensionFilterMode, extensions: &[&str]) -> ExtensionFilter {
        ExtensionFilter::new(mode, FileExtensions::new(extensions))
    }

    #[test]
    fn new_normalizes_case_dots_whitespace_and_duplicates() {
        let extensions = FileExtensions::new([" .MD ", "md", "Lock", ""]);
        assert_eq!(extensions.extensions(), ["lock", "md"]);
    }

    #[test]
    fn deserialization_preserves_normalized_matching() {
        let extensions: FileExtensions = serde_json::from_str(r#"[".MD", "lock", "md"]"#).unwrap();
        assert_eq!(extensions.extensions(), ["lock", "md"]);
        assert!(extensions.contains_extension_of(&path("Cargo.lock")));
    }

    #[test]
    fn contains_final_extension_case_insensitively() {
        let extensions = FileExtensions::new(["md"]);
        assert!(extensions.contains_extension_of(&path("README.md")));
        assert!(extensions.contains_extension_of(&path("docs/adr/0001.MD")));
        assert!(!extensions.contains_extension_of(&path("src/md/main.rs")));
    }

    #[test]
    fn contains_only_the_final_extension_of_compound_names() {
        let extensions = FileExtensions::new(["gz"]);
        assert!(extensions.contains_extension_of(&path("dist/bundle.tar.gz")));
        assert!(!FileExtensions::new(["tar"]).contains_extension_of(&path("dist/bundle.tar.gz")));
    }

    #[test]
    fn extensionless_and_dotfile_paths_are_never_listed() {
        let extensions = FileExtensions::new(["gitignore", "makefile"]);
        assert!(!extensions.contains_extension_of(&path(".gitignore")));
        assert!(!extensions.contains_extension_of(&path("Makefile")));
    }

    #[test]
    fn hide_mode_hides_listed_extensions() {
        let hide = filter(ExtensionFilterMode::Hide, &["lock"]);
        assert!(hide.hides(&path("Cargo.lock")));
        assert!(!hide.hides(&path("src/main.rs")));
        assert!(!hide.hides(&path("Makefile")));
    }

    #[test]
    fn only_mode_hides_every_unlisted_file() {
        let only = filter(ExtensionFilterMode::Only, &["rs"]);
        assert!(!only.hides(&path("src/main.rs")));
        assert!(only.hides(&path("Cargo.lock")));
        assert!(only.hides(&path("Makefile")));
    }

    #[test]
    fn empty_filters_hide_nothing_in_either_mode() {
        for mode in [ExtensionFilterMode::Hide, ExtensionFilterMode::Only] {
            let empty = filter(mode, &[]);
            assert!(!empty.is_active());
            assert!(!empty.hides(&path("README.md")));
            assert!(!empty.hides(&path("Makefile")));
        }
    }

    #[test]
    fn extensions_ignore_only_ascii_case_like_git_icase() {
        let extensions = FileExtensions::new(["ÄB"]);
        assert_eq!(extensions.extensions(), ["Äb"]);
        assert!(extensions.contains_extension_of(&path("x.ÄB")));
        assert!(!extensions.contains_extension_of(&path("x.äb")));
    }

    #[test]
    fn filter_selections_agree_with_hides() {
        let paths = [
            "src/main.rs",
            "Cargo.LOCK",
            ".lock",
            "..lock",
            "dir.lock/main.rs",
            "Makefile",
        ]
        .map(path);
        let assert_agreement = |filter: ExtensionFilter| {
            for path in &paths {
                assert_eq!(filter.shown().contains(path), !filter.hides(path));
                assert_eq!(filter.hidden().contains(path), filter.hides(path));
            }
        };
        for mode in [ExtensionFilterMode::Hide, ExtensionFilterMode::Only] {
            for extensions in [&[][..], &["lock"], &["lock", "rs"]] {
                assert_agreement(filter(mode, extensions));
            }
        }
    }

    #[test]
    fn selection_intersections_stay_single_lists() {
        let listed =
            |extensions: &[&str]| ExtensionSelection::Listed(FileExtensions::new(extensions));
        let unlisted =
            |extensions: &[&str]| ExtensionSelection::Unlisted(FileExtensions::new(extensions));

        assert_eq!(
            listed(&["lock", "md"]).intersection(&listed(&["md", "rs"])),
            listed(&["md"])
        );
        assert_eq!(
            listed(&["lock", "md"]).intersection(&unlisted(&["md"])),
            listed(&["lock"])
        );
        assert_eq!(
            unlisted(&["md"]).intersection(&listed(&["lock", "md"])),
            listed(&["lock"])
        );
        assert_eq!(
            unlisted(&["lock"]).intersection(&unlisted(&["md"])),
            unlisted(&["lock", "md"])
        );
        assert_eq!(
            ExtensionSelection::all().intersection(&listed(&["rs"])),
            listed(&["rs"])
        );
    }

    #[test]
    fn modes_round_trip_through_their_tokens() {
        for mode in <ExtensionFilterMode as strum::VariantArray>::VARIANTS {
            assert_eq!(mode.as_str().parse(), Ok(*mode));
        }
        assert!("show".parse::<ExtensionFilterMode>().is_err());
    }

    #[test]
    fn applied_filters_require_at_least_one_hidden_path() {
        let hide = filter(ExtensionFilterMode::Hide, &["md"]);
        assert_eq!(AppliedExtensionFilter::from_hidden(&hide, Vec::new()), None);

        let applied = AppliedExtensionFilter::from_hidden(&hide, vec![path("README.md")]).unwrap();
        assert_eq!(applied.filter, hide);
        assert_eq!(applied.hidden_paths, [path("README.md")]);
        assert_eq!(applied.extensions_label(), "md");
    }
}
