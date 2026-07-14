use super::{commit::Commit, exclusions::AppliedExclusions, file::FileDiff};

/// The `$ <lead><range><trail>` command line shown at the top of the screen.
#[derive(Debug, Clone)]
pub struct Cmd {
    pub lead: String,
    pub range: String,
    pub trail: String,
}

/// The footer prompt line (command + muted note).
#[derive(Debug, Clone)]
pub struct Foot {
    pub cmd: String,
    pub note: String,
}

/// Everything `build_html` needs. In JS these were the destructured params of
/// `buildHtml({...})`, with defaults the caller supplies before rendering.
#[derive(Debug, Clone)]
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
    /// Diff-preview theme read from config; `None` = default.
    pub theme: Option<String>,
    /// `Some` when the config's `[diff.exclude]` filter hid files from this
    /// view — every surface must show it so hidden files never read as missing.
    pub exclusions: Option<AppliedExclusions>,
}

impl View {
    /// Whether the preview would show nothing: no commits in range and no changed
    /// files. Rendering this case produces a blank artifact that reads as a bug, so
    /// callers warn and skip the render instead.
    pub fn is_empty(&self) -> bool {
        self.commits.is_empty() && self.files.is_empty()
    }
}

/// Reorders changed files into directory-tree order: at each directory level,
/// subdirectories come before files, both sorted alphabetically (depth-first).
/// This is the single source of truth for file order; the sidebar follows it.
pub fn sort_files_tree_order(files: &mut [FileDiff]) {
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
    use super::{super::file::LineOwners, *};

    fn file(path: &str) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            added: 0,
            removed: 0,
            lines: Vec::new(),
            full_lines: None,
            commits: Vec::new(),
            owners: LineOwners::default(),
        }
    }

    #[test]
    fn sort_files_tree_order_dirs_before_files_alpha_per_level() {
        let mut files = vec![
            file("src/render.rs"),
            file("docs/adr/0001-render-stack.md"),
            file("src/assets/preview.css"),
            file("src/model.rs"),
            file("src/assets/components.js"),
        ];

        sort_files_tree_order(&mut files);

        let order: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "docs/adr/0001-render-stack.md",
                "src/assets/components.js",
                "src/assets/preview.css",
                "src/model.rs",
                "src/render.rs",
            ]
        );
    }
}
