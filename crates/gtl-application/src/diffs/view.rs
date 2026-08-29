use gtl_models::{
    diffs::{AppliedExclusions, Commit},
    git::{GitHead, GitRevision},
    paths::{ProjectName, RepositoryRoot},
};

use super::file::FileDiff;

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

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub repo_name: ProjectName,
    pub repo_root: RepositoryRoot,
    pub branch: GitHead,
    pub upstream: GitRevision,
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub title: String,
    pub cmd: Cmd,
    pub commits_label: String,
    pub foot: Foot,
    /// Renderers must disclose hidden files when this is present.
    pub exclusions: Option<AppliedExclusions>,
}

impl View {
    #[must_use]
    pub fn has_diff_content(&self) -> bool {
        !self.commits.is_empty() || !self.files.is_empty()
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
            lines: Vec::new(),
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
    fn file_tree_order_places_directories_before_files_at_each_level() {
        let mut files = vec![
            FileDiff {
                path: repository_relative_path("src/render.rs"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("docs/adr/0001-render-stack.md"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/assets/preview.css"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/model.rs"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/assets/components.js"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
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
