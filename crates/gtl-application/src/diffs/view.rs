use gtl_models::{
    diffs::{
        AppliedExtensionFilter, Commit, DiffReviewScope, DiffTextId, DiffViewTitle,
        ExtensionSelection,
    },
    git::{GitDiffSpec, GitHead, GitRevision},
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
};

use super::file::FileDiff;
use crate::ports::{GitDiffFormat, GitDiffRequest};

/// The `$ <lead><range><trail>` command line shown at the top of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Cmd {
    pub lead: String,
    pub range: String,
    pub trail: String,
}

/// The footer prompt line.
#[derive(Debug, Clone, PartialEq)]
pub struct Foot {
    pub cmd: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullContextDiffState {
    Unavailable,
    Deferred(FullContextDiffSource),
    Loaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullContextDiffSource {
    git_request: GitDiffRequest,
}

impl FullContextDiffSource {
    #[must_use]
    pub fn new(spec: GitDiffSpec, paths: ExtensionSelection) -> Self {
        Self {
            git_request: GitDiffRequest {
                spec,
                format: GitDiffFormat::FullContext,
                paths,
            },
        }
    }

    pub(crate) fn git_request(&self) -> &GitDiffRequest {
        &self.git_request
    }

    pub(crate) fn spec(&self) -> &GitDiffSpec {
        &self.git_request.spec
    }

    pub(crate) fn paths(&self) -> &ExtensionSelection {
        &self.git_request.paths
    }
}

/// Parsed source returned by an explicit full-context fetch.
#[derive(Debug, PartialEq)]
pub struct FullContextDiff {
    pub(in crate::diffs) files: Vec<FileDiff>,
}

impl FullContextDiff {
    pub(in crate::diffs) fn from_files(files: Vec<FileDiff>) -> Self {
        Self { files }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FullContextDiffTransitionError {
    #[error("full-context diff source is unavailable")]
    Unavailable,
    #[error("full-context diff is already loaded")]
    AlreadyLoaded,
}

/// The repository checkout a view compares.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryOrigin {
    pub name: ProjectName,
    pub root: RepositoryRoot,
    pub branch: GitHead,
    pub upstream: GitRevision,
}

/// Stored diff text a view shows without a repository.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOrigin {
    pub label: ProjectName,
    pub id: DiffTextId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewOrigin {
    Repository(RepositoryOrigin),
    Text(TextOrigin),
}

impl ViewOrigin {
    /// The repository name, or the label of stored text.
    #[must_use]
    pub fn name(&self) -> &ProjectName {
        match self {
            Self::Repository(repository) => &repository.name,
            Self::Text(text) => &text.label,
        }
    }

    #[must_use]
    pub fn repository(&self) -> Option<&RepositoryOrigin> {
        match self {
            Self::Repository(repository) => Some(repository),
            Self::Text(_) => None,
        }
    }

    #[must_use]
    pub fn review_scope(&self) -> DiffReviewScope {
        match self {
            Self::Repository(repository) => DiffReviewScope::Repository(repository.root.clone()),
            Self::Text(_) => DiffReviewScope::Text,
        }
    }

    pub fn repository_mut(&mut self) -> Option<&mut RepositoryOrigin> {
        match self {
            Self::Repository(repository) => Some(repository),
            Self::Text(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub file_filter: super::file_filter::DiffFileFilter,
    pub origin: ViewOrigin,
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub title: DiffViewTitle,
    pub cmd: Cmd,
    pub foot: Foot,
    pub full_context: FullContextDiffState,
    /// Renderers must disclose hidden files when this is present.
    pub extension_filter: Option<AppliedExtensionFilter>,
}

impl View {
    #[must_use]
    pub fn has_diff_content(&self) -> bool {
        !self.commits.is_empty() || !self.files.is_empty() || self.extension_filter.is_some()
    }

    /// Applies fetched full-context source to a deferred view.
    pub fn with_full_context(
        mut self,
        full_context: FullContextDiff,
    ) -> Result<Self, FullContextDiffTransitionError> {
        match &self.full_context {
            FullContextDiffState::Unavailable => {
                return Err(FullContextDiffTransitionError::Unavailable);
            }
            FullContextDiffState::Loaded => {
                return Err(FullContextDiffTransitionError::AlreadyLoaded);
            }
            FullContextDiffState::Deferred(_) => {}
        }

        attach_full_context(&mut self.files, full_context);
        self.full_context = FullContextDiffState::Loaded;
        Ok(self)
    }
}

pub(super) fn attach_full_context(files: &mut [FileDiff], full_context: FullContextDiff) {
    let mut full_by_path: std::collections::HashMap<
        RepositoryRelativePath,
        super::source_lines::DiffSourceLines,
    > = full_context
        .files
        .into_iter()
        .map(|file| (file.path, file.lines))
        .collect();
    for file in files {
        if file.status() != super::FileStatus::Modified {
            continue;
        }
        let Some(full_lines) = full_by_path.remove(&file.path) else {
            continue;
        };
        if full_lines != file.lines {
            file.full_lines = Some(full_lines);
        }
    }
}

pub(in crate::diffs) fn sort_files_tree_order(files: &mut [FileDiff]) {
    files.sort_by(compare_file_tree_order);
}

fn compare_file_tree_order(a: &FileDiff, b: &FileDiff) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a_components = a.path.components().collect::<Vec<_>>();
    let b_components = b.path.components().collect::<Vec<_>>();
    let Some(index) = a_components
        .iter()
        .zip(&b_components)
        .position(|(a, b)| a != b)
    else {
        return a_components.len().cmp(&b_components.len());
    };
    match (
        index < a_components.len() - 1,
        index < b_components.len() - 1,
    ) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a_components[index].cmp(&b_components[index]),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};

    use super::*;
    use crate::utils::diffs::{commit, view};

    fn repository_relative_path(path: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(path.into()).unwrap()
    }

    fn file() -> FileDiff {
        FileDiff {
            path: repository_relative_path("f.txt"),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::default(),
            lines: crate::diffs::source_lines::DiffSourceLines::default(),
            full_lines: None,
        }
    }

    #[test]
    fn diff_content_requires_a_commit_or_changed_file() {
        let mut view = view();
        assert!(!view.has_diff_content());

        view.commits.push(commit("abc1234"));
        assert!(view.has_diff_content());

        view.commits.clear();
        view.files.push(file());
        assert!(view.has_diff_content());
    }

    #[test]
    fn full_context_transition_rejects_a_view_without_deferred_source() {
        let mut view = view();
        view.full_context = FullContextDiffState::Loaded;

        let error = view
            .with_full_context(FullContextDiff::from_files(Vec::new()))
            .unwrap_err();

        assert_eq!(error, FullContextDiffTransitionError::AlreadyLoaded);
    }

    #[test]
    fn full_context_transition_rejects_an_unavailable_source() {
        let error = view()
            .with_full_context(FullContextDiff::from_files(Vec::new()))
            .unwrap_err();

        assert_eq!(error, FullContextDiffTransitionError::Unavailable);
    }

    #[test]
    fn full_context_transition_updates_only_modified_files_without_reordering() {
        let compact = crate::diffs::unified_diff::parse(
            "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+new file\n",
        )
        .unwrap();
        let full = crate::diffs::unified_diff::parse(
            "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,3 +1,3 @@\n\
 before\n\
-old\n\
+new\n\
 after\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+new file\n",
        )
        .unwrap();
        let mut view = view();
        view.files = compact;
        view.full_context = FullContextDiffState::Deferred(FullContextDiffSource::new(
            gtl_models::git::GitDiffSpec::Range(crate::utils::git_range("a..b")),
            ExtensionSelection::all(),
        ));

        let view = view
            .with_full_context(FullContextDiff::from_files(full))
            .unwrap();

        assert!(matches!(view.full_context, FullContextDiffState::Loaded));
        assert_eq!(
            view.files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["f.txt", "g.txt"]
        );
        assert!(
            view.files[0]
                .full_lines
                .as_ref()
                .is_some_and(|lines| lines.iter().any(|line| line == "before"))
        );
        assert!(view.files[1].full_lines.is_none());
    }

    #[test]
    fn file_tree_order_places_directories_before_files_at_each_level() {
        let mut files = vec![
            FileDiff {
                path: repository_relative_path("src/render.rs"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("docs/adr/0001-render-stack.md"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/assets/preview.css"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/model.rs"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/assets/components.js"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            },
        ];

        sort_files_tree_order(&mut files);

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            [
                "docs/adr/0001-render-stack.md".to_owned(),
                "src/assets/components.js".to_owned(),
                "src/assets/preview.css".to_owned(),
                "src/model.rs".to_owned(),
                "src/render.rs".to_owned(),
            ]
        );
    }
}
