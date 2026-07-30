use std::{collections::HashMap, path::Path};

use domain::diffs::{Commit, ExcludedExtensions};

use crate::{
    diffs::{FileDiff, FileStatus, UnifiedDiffLineClassifier, UnifiedDiffLineKind, View},
    ports::{GitClient, GitDiffFormat, GitDiffRequest},
    shared::notes::Note,
};

pub fn parse_diff(raw: &str) -> Vec<FileDiff> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut line_classifier = UnifiedDiffLineClassifier::default();

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git a/")
            && let Some((_, path)) = rest.split_once(" b/")
        {
            if let Some(file) = cur.take() {
                files.push(file);
            }
            line_classifier = UnifiedDiffLineClassifier::default();
            cur = Some(FileDiff {
                path: path.to_string(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            });
            continue;
        }

        let Some(file) = cur.as_mut() else {
            continue;
        };

        file.lines.push(line.to_string());
        match line_classifier.classify(line) {
            UnifiedDiffLineKind::Added => file.added += 1,
            UnifiedDiffLineKind::Removed => file.removed += 1,
            UnifiedDiffLineKind::Meta
            | UnifiedDiffLineKind::Hunk { .. }
            | UnifiedDiffLineKind::Context => {}
        }
    }

    if let Some(file) = cur {
        files.push(file);
    }

    files
}

pub fn attach_full_context(files: &mut [FileDiff], full_files: Vec<FileDiff>) {
    let mut full_by_path: HashMap<String, Vec<String>> = full_files
        .into_iter()
        .map(|file| (file.path, file.lines))
        .collect();

    for file in files {
        if file.status() != FileStatus::Modified {
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

/// The assembled diff data for one preview: commits, changed files, and hidden paths.
pub struct DiffData {
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub hidden_paths: Vec<String>,
}

/// The shared diff generator: log + diff + exclusion filter + full context.
/// Does not sort files; callers retain ownership of presentation order.
///
/// Exclusions are applied *before* the content diffs run: a cheap `--name-only`
/// pass discovers the hidden paths, and both content invocations then carry
/// `:(exclude,literal)` pathspecs, so git never computes — and this module
/// never parses or counts — an excluded file's line diffs.
pub fn assemble(
    source: &impl GitClient,
    repo: &Path,
    diff_range: &str,
    log_range: &str,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<DiffData> {
    let commits = source.log_commits(repo, log_range)?;

    let hidden_paths = hidden_paths(source, repo, diff_range, excluded)?;
    let content_request = GitDiffRequest {
        range: diff_range.to_string(),
        format: GitDiffFormat::Unified,
        excluded_paths: hidden_paths.clone(),
    };
    // ? partition again after parsing: a source that ignores the exclude
    // ? pathspecs (the scripted test fake) must still never leak hidden files.
    let (mut files, _) =
        partition_excluded(parse_diff(&source.diff(repo, &content_request)?), excluded);
    let full_context_request = GitDiffRequest {
        format: GitDiffFormat::FullContext,
        ..content_request
    };
    attach_full_context(
        &mut files,
        parse_diff(&source.diff(repo, &full_context_request)?),
    );
    Ok(DiffData {
        commits,
        files,
        hidden_paths,
    })
}

/// The paths the exclusion set hides, in diff order, discovered through a
/// content-free `--name-only` pass. Skips the extra git call entirely when
/// nothing is excluded.
fn hidden_paths(
    source: &impl GitClient,
    repo: &Path,
    range: &str,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<Vec<String>> {
    if excluded.is_empty() {
        return Ok(Vec::new());
    }
    Ok(source
        .diff(
            repo,
            &GitDiffRequest {
                range: range.to_string(),
                format: GitDiffFormat::NamesOnly,
                excluded_paths: Vec::new(),
            },
        )?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty() && excluded.matches(path))
        .map(String::from)
        .collect())
}

/// Split parsed files into (kept, hidden paths) under the exclusion set (pure).
fn partition_excluded(
    files: Vec<FileDiff>,
    excluded: &ExcludedExtensions,
) -> (Vec<FileDiff>, Vec<String>) {
    if excluded.is_empty() {
        return (files, Vec::new());
    }
    let (hidden, kept): (Vec<FileDiff>, Vec<FileDiff>) = files
        .into_iter()
        .partition(|file| excluded.matches(&file.path));
    (kept, hidden.into_iter().map(|file| file.path).collect())
}

/// The user-facing note for a view whose exclusion filter hid files, prefixed
/// with the slice's label (`diff-preview`, `squash-preview`).
pub(crate) fn exclusion_note(label: &str, view: &View) -> Option<Note> {
    view.exclusions.as_ref().map(|applied| {
        Note::info(format!(
            "{label}: {} file(s) hidden by config [diff.exclude] ({})",
            applied.hidden_paths.len(),
            applied.extensions_label(),
        ))
    })
}

/// The repo's display name: the last path component of its git top-level, falling
/// back to `"repo"` when the path has no usable final component.
pub(crate) fn repo_name(top: &str) -> String {
    Path::new(top)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
index 000..333\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+brand new\n";

    #[test]
    fn parse_diff_splits_two_file_diff_and_counts_body_changes() {
        let files = parse_diff(SAMPLE);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "f.txt");
        assert_eq!(files[0].added, 2);
        assert_eq!(files[0].removed, 1);
        assert_eq!(files[1].path, "g.txt");
        assert_eq!(files[1].added, 1);
        assert_eq!(files[1].removed, 0);
    }

    #[test]
    fn parse_diff_does_not_count_file_headers_as_changes() {
        let files = parse_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, 0);
        assert_eq!(files[0].removed, 0);
    }

    #[test]
    fn parse_diff_counts_header_like_hunk_content() {
        let files = parse_diff(
            "diff --git a/a.sql b/a.sql\n\
index 111..222 100644\n\
--- a/a.sql\n\
+++ b/a.sql\n\
@@ -1,2 +1,3 @@\n\
--- old heading\n\
+-- new heading\n\
+++ literal\n\
 keep\n",
        );

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].removed, 1);
        assert_eq!(files[0].added, 2);
    }

    #[test]
    fn parse_diff_returns_empty_for_blank_input() {
        assert!(parse_diff("").is_empty());
        assert!(parse_diff("   \n\t").is_empty());
    }

    #[test]
    fn attach_full_context_only_sets_modified_files_with_extra_context() {
        let mut files = parse_diff(SAMPLE);
        let full = parse_diff(
            "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,4 +1,5 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n\
 middle\n\
 end\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
index 000..333\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+brand new\n",
        );

        attach_full_context(&mut files, full);

        assert!(
            files[0]
                .full_lines
                .as_ref()
                .is_some_and(|lines| { lines.iter().any(|line| line == "middle") })
        );
        assert!(files[1].full_lines.is_none());
    }
}
