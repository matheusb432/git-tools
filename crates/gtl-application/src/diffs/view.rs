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

/// The complete diff view consumed by renderers and viewer projections.
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
    /// `Some` when the config's `[diff.exclude]` filter hid files from this
    /// view — every surface must show it so hidden files never read as missing.
    pub exclusions: Option<AppliedExclusions>,
}

impl View {
    /// Returns whether the view contains at least one commit or changed file.
    ///
    /// Snapshot artifacts require diff content. Live views may remain open without it so they can
    /// be refreshed later.
    pub fn has_diff_content(&self) -> bool {
        !self.commits.is_empty() || !self.files.is_empty()
    }
}

/// Reorders changed files into directory-tree order: at each directory level,
/// subdirectories come before files, both sorted alphabetically (depth-first).
/// This is the single source of truth for file order; the sidebar follows it.
pub(in crate::diffs) fn sort_files_tree_order(files: &mut [FileDiff]) {
    use std::cmp::Ordering;

    files.sort_by(|a, b| {
        let a_components: Vec<_> = a.path.components().collect();
        let b_components: Vec<_> = b.path.components().collect();

        for i in 0..a_components.len().min(b_components.len()) {
            if a_components[i] == b_components[i] {
                continue;
            }
            // ? a component that is not the last in its path is a directory name
            let a_is_dir = i < a_components.len() - 1;
            let b_is_dir = i < b_components.len() - 1;
            return match (a_is_dir, b_is_dir) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => a_components[i].cmp(&b_components[i]),
            };
        }
        // ? one path is a prefix of the other: shorter (shallower) comes first
        a_components.len().cmp(&b_components.len())
    });
}

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};

    use super::*;
    use crate::utils::diffs::{commit, view};

    fn repository_relative_path(path: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(path.into()).expect("repository-relative path")
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
