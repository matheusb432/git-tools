use gtl_models::diffs::{AppliedExclusions, Commit};

use super::file::FileDiff;

/// The `$ <lead><range><trail>` command line shown at the top of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Cmd {
    pub lead: String,
    pub range: String,
    pub trail: String,
}

/// The footer prompt line (command + muted note).
#[derive(Debug, Clone, PartialEq)]
pub struct Foot {
    pub cmd: String,
    pub note: String,
}

/// Everything `build_html` needs. In JS these were the destructured params of
/// `buildHtml({...})`, with defaults the caller supplies before rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub repo_name: String,
    /// Absolute path to the repo root (git top-level), used to compose copy-able
    /// absolute file paths in the preview. POSIX-joined with `path` at render time.
    pub repo_root: String,
    pub branch: String,
    pub upstream: String,
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
    /// Snapshot previews require diff content. Live views may remain open without it so they can be
    /// refreshed later.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gtl_application::diffs::{Cmd, Foot, View};
    /// # let view = View {
    /// #     repo_name: "repo".into(),
    /// #     repo_root: "/repo".into(),
    /// #     branch: "feature".into(),
    /// #     upstream: "origin/main".into(),
    /// #     commits: Vec::new(),
    /// #     files: Vec::new(),
    /// #     title: "Diff".into(),
    /// #     cmd: Cmd { lead: String::new(), range: String::new(), trail: String::new() },
    /// #     commits_label: "Commits".into(),
    /// #     foot: Foot { cmd: "git diff".into(), note: String::new() },
    /// #     exclusions: None,
    /// # };
    /// assert!(!view.has_diff_content());
    /// ```
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
        let a_components: Vec<&str> = a.path.split('/').collect();
        let b_components: Vec<&str> = b.path.split('/').collect();

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
                _ => a_components[i].cmp(b_components[i]),
            };
        }
        // ? one path is a prefix of the other: shorter (shallower) comes first
        a_components.len().cmp(&b_components.len())
    });
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::Commit;

    use super::*;

    fn view() -> View {
        View {
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "origin/main".into(),
            commits: Vec::new(),
            files: Vec::new(),
            title: "Diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: "Commits".into(),
            foot: Foot {
                cmd: "git diff".into(),
                note: String::new(),
            },
            exclusions: None,
        }
    }

    fn file() -> FileDiff {
        FileDiff {
            path: "f.txt".into(),
            added: 1,
            removed: 0,
            lines: Vec::new(),
            full_lines: None,
        }
    }

    #[test]
    fn diff_content_requires_a_commit_or_changed_file() {
        let mut view = view();
        assert!(!view.has_diff_content());

        view.commits.push(Commit::default());
        assert!(view.has_diff_content());

        view.commits.clear();
        view.files.push(file());
        assert!(view.has_diff_content());
    }

    #[test]
    fn file_tree_order_places_directories_before_files_at_each_level() {
        let mut files = vec![
            FileDiff {
                path: "src/render.rs".into(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: "docs/adr/0001-render-stack.md".into(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: "src/assets/preview.css".into(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: "src/model.rs".into(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: "src/assets/components.js".into(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
        ];

        sort_files_tree_order(&mut files);

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            [
                "docs/adr/0001-render-stack.md",
                "src/assets/components.js",
                "src/assets/preview.css",
                "src/model.rs",
                "src/render.rs",
            ]
        );
    }
}
