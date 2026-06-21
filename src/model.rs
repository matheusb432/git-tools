//! Data model for the diff-preview rendering pipeline.
//! Data shapes flowing through the diff-preview rendering pipeline.

use std::collections::HashMap;

/// One changed file: its path, +/- counts, raw diff lines, the short shas of the
/// commits that touched it (drives the file filter), and per-line ownership
/// (drives the per-commit line highlight).
#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub lines: Vec<String>,
    pub full_lines: Option<Vec<String>>,
    pub commits: Vec<String>,
    pub owners: LineOwners,
}

/// Per-line commit ownership for one file, keyed by absolute line number:
/// `added` by new-side line, `deleted` by old-side line. The same maps serve
/// both the compact and full-file panes — changed rows keep absolute numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineOwners {
    pub added: HashMap<u32, String>,
    pub deleted: HashMap<u32, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileStatus {
    Added,
    Deleted,
    Renamed,
    Modified,
}

impl FileStatus {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Modified => "modified",
        }
    }

    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Modified => "M",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Added => "Added file",
            Self::Deleted => "Deleted file",
            Self::Renamed => "Renamed file",
            Self::Modified => "Modified file",
        }
    }

    pub(crate) fn css_class(self) -> &'static str {
        match self {
            Self::Added => "status-added",
            Self::Deleted => "status-deleted",
            Self::Renamed => "status-renamed",
            Self::Modified => "status-modified",
        }
    }
}

impl FileDiff {
    pub(crate) fn status(&self) -> FileStatus {
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("rename from ") || line.starts_with("rename to "))
        {
            return FileStatus::Renamed;
        }
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("new file ") || line == "--- /dev/null")
        {
            return FileStatus::Added;
        }
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("deleted file ") || line == "+++ /dev/null")
        {
            return FileStatus::Deleted;
        }
        FileStatus::Modified
    }
}

/// One commit in range. `date`/`iso` are the human + machine timestamps.
/// `parents` are the short shas of its parents (≥2 ⇒ a merge); `members` are the
/// commits this merge brought into the previewed range (empty for a non-merge).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
    pub parents: Vec<String>,
    pub members: Vec<String>,
}

impl Commit {
    /// A commit with two or more parents is a merge.
    pub fn is_merge(&self) -> bool {
        self.parents.len() >= 2
    }
}

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
    use super::*;

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
    fn line_owners_default_is_empty() {
        let owners = LineOwners::default();
        assert!(owners.added.is_empty() && owners.deleted.is_empty());
    }

    #[test]
    fn is_merge_is_true_only_with_two_or_more_parents() {
        let mut commit = Commit::default();
        assert!(!commit.is_merge()); // 0 parents (root)
        commit.parents = vec!["p1aaaaaaa".to_string()];
        assert!(!commit.is_merge()); // 1 parent (normal)
        commit.parents.push("p2bbbbbbb".to_string());
        assert!(commit.is_merge()); // 2 parents (merge)
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
