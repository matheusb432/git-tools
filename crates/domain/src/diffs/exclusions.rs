//! Per-project diff exclusions by file extension.
//!
//! Configured centrally (the user config's `[diff.exclude]` table, keyed by
//! repo directory name), so a project never needs a config file in its own
//! working tree. The view carries an [`AppliedExclusions`] summary whenever
//! files were hidden, so every surface can show the filter instead of
//! silently dropping files.

use std::{collections::BTreeMap, path::Path};

/// The normalized extension set excluded for one project.
///
/// Extensions are stored lowercase without a leading dot; matching compares a
/// path's final extension case-insensitively, so `md`, `.MD`, and `Md` all
/// configure (and match) the same files. Files without an extension (e.g.
/// `Makefile`, `.gitignore`) never match.
///
/// # Examples
///
/// ```
/// use domain::diffs::ExcludedExtensions;
///
/// let excluded = ExcludedExtensions::new(["md", ".LOCK"]);
/// assert!(excluded.matches("docs/planning/plan.MD"));
/// assert!(excluded.matches("Cargo.lock"));
/// assert!(!excluded.matches("src/main.rs"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExcludedExtensions(Vec<String>);

impl ExcludedExtensions {
    /// Normalizes `raw` into a sorted, deduplicated extension set: trimmed,
    /// lowercased, leading dots stripped, empties dropped.
    pub fn new<I, S>(raw: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut extensions: Vec<String> = raw
            .into_iter()
            .map(|ext| ext.as_ref().trim().trim_start_matches('.').to_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect();
        extensions.sort();
        extensions.dedup();
        Self(extensions)
    }

    /// Whether no extension is excluded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `path`'s final extension is excluded (case-insensitive).
    pub fn matches(&self, path: &str) -> bool {
        if self.0.is_empty() {
            return false;
        }
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                let ext = ext.to_lowercase();
                self.0.binary_search(&ext).is_ok()
            })
    }

    /// The normalized extensions, sorted, for display.
    pub fn extensions(&self) -> &[String] {
        &self.0
    }
}

/// Every project's excluded extensions, keyed by repo directory name (the last
/// component of the repo's git top-level path).
///
/// # Examples
///
/// ```
/// use domain::diffs::DiffExclusions;
///
/// let exclusions = DiffExclusions::new([("git-tools".to_string(), vec!["md".to_string()])], None);
/// assert!(
///     exclusions
///         .for_project_or_default("git-tools")
///         .matches("README.md")
/// );
/// assert!(
///     !exclusions
///         .for_project_or_default("other")
///         .matches("README.md")
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffExclusions {
    projects: BTreeMap<String, ExcludedExtensions>,
    // TODO: use builder pattern
    default_exclusions: ExcludedExtensions,
}

impl DiffExclusions {
    pub const DEFAULT_KEY: &'static str = "defaults";

    /// Builds the map from raw `(project, extensions)` pairs, normalizing each
    /// set and dropping projects whose set normalizes to empty.
    pub fn new<I, S>(config_keys: I, default_key: Option<&'static str>) -> Self
    where
        I: IntoIterator<Item = (String, Vec<S>)>,
        S: AsRef<str>,
    {
        let default_key_effective = default_key.unwrap_or(Self::DEFAULT_KEY).to_string();
        let mut default_exclusions: Option<ExcludedExtensions> = None;

        Self {
            projects: config_keys
                .into_iter()
                .filter_map(|(project, raw)| {
                    // TODO: study cleaner way to do this
                    if project == default_key_effective {
                        default_exclusions = Some(ExcludedExtensions::new(raw));
                        None
                    } else {
                        Some((project, ExcludedExtensions::new(raw)))
                    }
                })
                .filter(|(_, extensions)| !extensions.is_empty())
                .collect(),
            default_exclusions: default_exclusions.unwrap_or_default(),
        }
    }

    /// The excluded extensions for `project`
    pub fn for_project(&self, project: &str) -> Option<&ExcludedExtensions> {
        self.projects.get(project)
    }

    /// The excluded extensions for `project` or the default exclusions
    pub fn for_project_or_default(&self, project: &str) -> &ExcludedExtensions {
        self.for_project(project)
            .unwrap_or(&self.default_exclusions)
    }

    /// Whether no project has exclusions.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }
}

/// The exclusion outcome a computed view carries so every surface (preview
/// chip, CLI note) can state what was hidden and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedExclusions {
    /// The configured extensions that were in force (normalized, sorted).
    pub extensions: Vec<String>,
    /// The paths hidden from the view, in diff order.
    pub hidden_paths: Vec<String>,
}

impl AppliedExclusions {
    /// `Some` summary only when the filter actually hid files — a configured
    /// but idle filter stays invisible.
    pub fn from_hidden(excluded: &ExcludedExtensions, hidden_paths: Vec<String>) -> Option<Self> {
        if hidden_paths.is_empty() {
            return None;
        }
        Some(Self {
            extensions: excluded.extensions().to_vec(),
            hidden_paths,
        })
    }

    /// The extensions joined for display: `"md, lock"`.
    pub fn extensions_label(&self) -> String {
        self.extensions.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_normalizes_case_dots_whitespace_and_duplicates() {
        let excluded = ExcludedExtensions::new([" .MD ", "md", "Lock", ""]);
        assert_eq!(excluded.extensions(), ["lock", "md"]);
    }

    #[test]
    fn matches_final_extension_case_insensitively() {
        let excluded = ExcludedExtensions::new(["md"]);
        assert!(excluded.matches("README.md"));
        assert!(excluded.matches("docs/adr/0001.MD"));
        assert!(!excluded.matches("src/md/main.rs"));
    }

    #[test]
    fn matches_only_the_final_extension_of_compound_names() {
        let excluded = ExcludedExtensions::new(["gz"]);
        assert!(excluded.matches("dist/bundle.tar.gz"));
        assert!(!ExcludedExtensions::new(["tar"]).matches("dist/bundle.tar.gz"));
    }

    #[test]
    fn extensionless_and_dotfile_paths_never_match() {
        let excluded = ExcludedExtensions::new(["gitignore", "makefile"]);
        assert!(!excluded.matches(".gitignore"));
        assert!(!excluded.matches("Makefile"));
    }

    #[test]
    fn empty_set_matches_nothing() {
        assert!(!ExcludedExtensions::default().matches("README.md"));
    }

    #[test]
    fn from_projects_drops_projects_that_normalize_to_empty() {
        let exclusions = DiffExclusions::new(
            [
                ("kept".to_string(), vec!["md"]),
                ("dropped".to_string(), vec!["", " . "]),
            ],
            None,
        );
        assert!(exclusions.for_project_or_default("kept").matches("a.md"));
        assert!(exclusions.for_project_or_default("dropped").is_empty());
    }

    #[test]
    fn from_projects_with_default_key_separates_default_from_projects() {
        let default_key = DiffExclusions::DEFAULT_KEY;
        let exclusions = DiffExclusions::new(
            [
                ("proj1".into(), vec!["md"]),
                (default_key.into(), vec!["ts", "py"]),
                ("proj2".into(), vec!["ts"]),
            ],
            Some(default_key),
        );
        assert!(exclusions.for_project_or_default("proj1").matches("a.md"));
        assert!(exclusions.for_project_or_default("proj2").matches("a.ts"));
        assert!(exclusions.for_project(default_key).is_none());
        assert!(exclusions.for_project("proj3").is_none());
        assert!(exclusions.for_project_or_default("proj3").matches("a.py"));
    }

    #[test]
    fn for_project_or_default_falls_back_to_the_empty_set() {
        assert!(
            DiffExclusions::default()
                .for_project_or_default("anything")
                .is_empty()
        );
    }

    #[test]
    fn applied_exclusions_require_at_least_one_hidden_path() {
        let excluded = ExcludedExtensions::new(["md"]);
        assert_eq!(AppliedExclusions::from_hidden(&excluded, Vec::new()), None);

        let applied = AppliedExclusions::from_hidden(&excluded, vec!["README.md".into()])
            .expect("hidden path yields a summary");
        assert_eq!(applied.extensions, ["md"]);
        assert_eq!(applied.hidden_paths, ["README.md"]);
        assert_eq!(applied.extensions_label(), "md");
    }
}
