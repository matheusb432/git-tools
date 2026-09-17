use std::{collections::BTreeMap, path::Path};

use crate::paths::{ProjectName, RepositoryRelativePath};

/// Lowercase, dotless, sorted, unique extensions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(from = "Vec<String>", into = "Vec<String>")]
pub struct ExcludedExtensions(Vec<String>);

impl From<Vec<String>> for ExcludedExtensions {
    fn from(raw: Vec<String>) -> Self {
        Self::new(raw)
    }
}

impl From<ExcludedExtensions> for Vec<String> {
    fn from(value: ExcludedExtensions) -> Self {
        value.0
    }
}

impl ExcludedExtensions {
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn matches(&self, path: &RepositoryRelativePath) -> bool {
        if self.0.is_empty() {
            return false;
        }
        Path::new(path.as_path())
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                let ext = ext.to_lowercase();
                self.0.binary_search(&ext).is_ok()
            })
    }

    #[must_use]
    pub fn extensions(&self) -> &[String] {
        &self.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffExclusions {
    projects: BTreeMap<ProjectName, ExcludedExtensions>,
    default_exclusions: ExcludedExtensions,
}

impl DiffExclusions {
    pub fn new<I, S>(project_exclusions: I, default_exclusions: Option<Vec<S>>) -> Self
    where
        I: IntoIterator<Item = (ProjectName, Vec<S>)>,
        S: AsRef<str>,
    {
        Self {
            projects: project_exclusions
                .into_iter()
                .map(|(project, raw)| (project, ExcludedExtensions::new(raw)))
                .collect(),
            default_exclusions: default_exclusions
                .map(ExcludedExtensions::new)
                .unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn for_project(&self, project: &ProjectName) -> Option<&ExcludedExtensions> {
        self.projects.get(project)
    }

    #[must_use]
    pub fn for_project_or_default(&self, project: &ProjectName) -> &ExcludedExtensions {
        self.for_project(project)
            .unwrap_or(&self.default_exclusions)
    }

    #[must_use]
    pub const fn default_exclusions(&self) -> &ExcludedExtensions {
        &self.default_exclusions
    }

    /// Iterates overrides in project-name order.
    #[must_use]
    pub fn project_exclusions(
        &self,
    ) -> impl ExactSizeIterator<Item = (&ProjectName, &ExcludedExtensions)> {
        self.projects.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.default_exclusions.is_empty()
            && self.projects.values().all(ExcludedExtensions::is_empty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedExclusions {
    pub extensions: ExcludedExtensions,
    pub hidden_paths: Vec<RepositoryRelativePath>,
}

impl AppliedExclusions {
    #[must_use]
    pub fn from_hidden(
        excluded: &ExcludedExtensions,
        hidden_paths: Vec<RepositoryRelativePath>,
    ) -> Option<Self> {
        if hidden_paths.is_empty() {
            return None;
        }
        Some(Self {
            extensions: excluded.clone(),
            hidden_paths,
        })
    }

    #[must_use]
    pub fn extensions_label(&self) -> String {
        self.extensions.extensions().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> ProjectName {
        ProjectName::try_new(name.to_owned()).unwrap()
    }

    fn path(value: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(value.into()).unwrap()
    }

    #[test]
    fn new_normalizes_case_dots_whitespace_and_duplicates() {
        let excluded = ExcludedExtensions::new([" .MD ", "md", "Lock", ""]);
        assert_eq!(excluded.extensions(), ["lock", "md"]);
    }

    #[test]
    fn deserialization_preserves_normalized_matching() {
        let excluded: ExcludedExtensions =
            serde_json::from_str(r#"[".MD", "lock", "md"]"#).unwrap();
        assert_eq!(excluded.extensions(), ["lock", "md"]);
        assert!(excluded.matches(&path("Cargo.lock")));
    }

    #[test]
    fn matches_final_extension_case_insensitively() {
        let excluded = ExcludedExtensions::new(["md"]);
        assert!(excluded.matches(&path("README.md")));
        assert!(excluded.matches(&path("docs/adr/0001.MD")));
        assert!(!excluded.matches(&path("src/md/main.rs")));
    }

    #[test]
    fn matches_only_the_final_extension_of_compound_names() {
        let excluded = ExcludedExtensions::new(["gz"]);
        assert!(excluded.matches(&path("dist/bundle.tar.gz")));
        assert!(!ExcludedExtensions::new(["tar"]).matches(&path("dist/bundle.tar.gz")));
    }

    #[test]
    fn extensionless_and_dotfile_paths_never_match() {
        let excluded = ExcludedExtensions::new(["gitignore", "makefile"]);
        assert!(!excluded.matches(&path(".gitignore")));
        assert!(!excluded.matches(&path("Makefile")));
    }

    #[test]
    fn empty_set_matches_nothing() {
        assert!(!ExcludedExtensions::default().matches(&path("README.md")));
    }

    #[test]
    fn from_projects_retains_an_explicit_empty_override() {
        let exclusions = DiffExclusions::new(
            [
                (project("kept"), vec!["md"]),
                (project("dropped"), vec!["", " . "]),
            ],
            Some(vec!["txt"]),
        );
        assert!(
            exclusions
                .for_project_or_default(&project("kept"))
                .matches(&path("a.md"))
        );
        assert!(
            exclusions
                .for_project_or_default(&project("dropped"))
                .is_empty()
        );
        assert!(exclusions.for_project(&project("dropped")).is_some());
    }

    #[test]
    fn from_projects_keeps_default_exclusions_separate() {
        let exclusions = DiffExclusions::new(
            [
                (project("proj1"), vec!["md"]),
                (project("proj2"), vec!["ts"]),
            ],
            Some(vec!["ts", "py"]),
        );
        assert!(
            exclusions
                .for_project_or_default(&project("proj1"))
                .matches(&path("a.md"))
        );
        assert!(
            exclusions
                .for_project_or_default(&project("proj2"))
                .matches(&path("a.ts"))
        );
        assert!(exclusions.for_project(&project("proj3")).is_none());
        assert!(
            exclusions
                .for_project_or_default(&project("proj3"))
                .matches(&path("a.py"))
        );
    }

    #[test]
    fn for_project_or_default_falls_back_to_the_empty_set() {
        assert!(
            DiffExclusions::default()
                .for_project_or_default(&project("anything"))
                .is_empty()
        );
    }

    #[test]
    fn read_accessors_expose_sorted_projects_and_separate_defaults() {
        let exclusions = DiffExclusions::new(
            [
                (project("zeta"), vec!["rs"]),
                (project("alpha"), vec!["toml"]),
            ],
            Some(vec!["md"]),
        );

        assert_eq!(exclusions.default_exclusions().extensions(), ["md"]);
        assert_eq!(
            exclusions
                .project_exclusions()
                .map(|(project, extensions)| {
                    (
                        project.as_str(),
                        extensions
                            .extensions()
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
            [("alpha", vec!["toml"]), ("zeta", vec!["rs"])]
        );
    }

    #[test]
    fn applied_exclusions_require_at_least_one_hidden_path() {
        let excluded = ExcludedExtensions::new(["md"]);
        assert_eq!(AppliedExclusions::from_hidden(&excluded, Vec::new()), None);

        let applied = AppliedExclusions::from_hidden(&excluded, vec![path("README.md")]).unwrap();
        assert_eq!(applied.extensions.extensions(), ["md"]);
        assert_eq!(applied.hidden_paths, [path("README.md")]);
        assert_eq!(applied.extensions_label(), "md");
    }
}
