use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use domain::diffs::{Commit, ExcludedExtensions};

use crate::{
    diffs::{
        FileDiff, FileStatus, LineOwners, UnifiedDiffLineClassifier, UnifiedDiffLineKind, View,
        attribution::{self, NewSide},
    },
    ports::DiffSource,
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
                commits: Vec::new(),
                owners: LineOwners::default(),
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

/// Attach each file's touching commits from the range's file→commits map (pure).
pub fn attach_commits<S: std::hash::BuildHasher>(
    files: &mut [FileDiff],
    map: &HashMap<String, Vec<String>, S>,
) {
    for file in files {
        file.commits = map.get(&file.path).cloned().unwrap_or_default();
    }
}

/// The assembled diff data for one preview: commits in range and changed files
/// (with per-line owners attached), plus the paths hidden by the extension
/// exclusion filter.
pub struct DiffData {
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub hidden_paths: Vec<String>,
}

/// The shared diff generator: log + diff + exclusion filter + full-context +
/// file-commit map + per-line attribution. Does NOT sort files — callers order
/// as they always have.
///
/// Exclusions are applied *before* the content diffs run: a cheap `--name-only`
/// pass discovers the hidden paths, and both content invocations then carry
/// `:(exclude,literal)` pathspecs, so git never computes — and this module
/// never parses, counts, or blames — an excluded file's line diffs.
pub fn assemble(
    source: &impl DiffSource,
    repo: &Path,
    diff_range: &str,
    log_range: &str,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<DiffData> {
    let diff_args = vec!["diff".to_string(), diff_range.to_string()];
    let mut commits = source.log_commits(repo, log_range)?;
    let (base, new_side) = blame_targets(diff_range, log_range);

    // ! Blame never attributes a line to a merge, so a merge card is otherwise dead. Map each
    // ! merge to the commits it brought into the range so focusing it lifts their rows.
    for commit in &mut commits {
        if commit.is_merge() {
            commit.members = source
                .merge_members(repo, &commit.sha, &base)
                .unwrap_or_default();
        }
    }

    let hidden_paths = hidden_paths(source, repo, &diff_args, excluded)?;
    let content_args = with_exclude_pathspecs(&diff_args, &hidden_paths);
    // ? partition again after parsing: a source that ignores the exclude
    // ? pathspecs (the scripted test fake) must still never leak hidden files.
    let (mut files, _) =
        partition_excluded(parse_diff(&source.diff_raw(repo, &content_args)?), excluded);
    attach_full_context(
        &mut files,
        parse_diff(&source.diff_raw(repo, &full_context_args(&content_args))?),
    );
    let file_commits = source.file_commit_map(repo, log_range)?;
    attach_commits(&mut files, &file_commits);

    let in_range: HashSet<String> = commits.iter().map(|commit| commit.sha.clone()).collect();
    attribution::attribute(source, repo, &base, &new_side, &in_range, &mut files);

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
    source: &impl DiffSource,
    repo: &Path,
    diff_args: &[String],
    excluded: &ExcludedExtensions,
) -> anyhow::Result<Vec<String>> {
    if excluded.is_empty() {
        return Ok(Vec::new());
    }
    Ok(source
        .diff_raw(repo, &name_only_args(diff_args))?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty() && excluded.matches(path))
        .map(String::from)
        .collect())
}

/// The `--name-only` variant of a diff invocation (mirrors [`full_context_args`]).
fn name_only_args(args: &[String]) -> Vec<String> {
    let mut name_only = Vec::with_capacity(args.len() + 1);
    if let Some((cmd, rest)) = args.split_first() {
        name_only.push(cmd.clone());
        name_only.push("--name-only".to_string());
        name_only.extend(rest.iter().cloned());
    }
    name_only
}

/// Append one `:(exclude,literal)` pathspec per hidden path so git never emits
/// (nor computes) those files' content. `literal` keeps glob characters in
/// filenames inert. No hidden paths → the args pass through untouched.
fn with_exclude_pathspecs(args: &[String], hidden_paths: &[String]) -> Vec<String> {
    if hidden_paths.is_empty() {
        return args.to_vec();
    }
    let mut excluded_args = Vec::with_capacity(args.len() + 1 + hidden_paths.len());
    excluded_args.extend(args.iter().cloned());
    excluded_args.push("--".to_string());
    excluded_args.extend(
        hidden_paths
            .iter()
            .map(|path| format!(":(exclude,literal){path}")),
    );
    excluded_args
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

// ! log_range is always two-dot `base..tip`; diff_range lacking `..` (hash mode) means the
// ! new side is the working tree, not a commit.
fn blame_targets(diff_range: &str, log_range: &str) -> (String, NewSide) {
    let base = log_range
        .split("..")
        .next()
        .unwrap_or(log_range)
        .to_string();
    let tip = log_range.rsplit("..").next().unwrap_or("HEAD").to_string();
    let new_side = if diff_range.contains("..") {
        NewSide::Commit(tip)
    } else {
        NewSide::WorkTree
    };
    (base, new_side)
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

pub fn full_context_args(args: &[String]) -> Vec<String> {
    let mut full_args = Vec::with_capacity(args.len() + 1);
    if let Some((cmd, rest)) = args.split_first() {
        full_args.push(cmd.clone());
        full_args.push("--unified=2147483647".to_string());
        full_args.extend(rest.iter().cloned());
    }
    full_args
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

    #[test]
    fn blame_targets_picks_base_tip_and_new_side() {
        let (base, side) = blame_targets("origin/main..HEAD", "origin/main..HEAD");
        assert_eq!(base, "origin/main");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "HEAD"));

        let (base, side) = blame_targets("main...HEAD", "main..HEAD"); // merge mode
        assert_eq!(base, "main");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "HEAD"));

        let (base, side) = blame_targets("abc123", "abc123..HEAD"); // hash mode -> worktree
        assert_eq!(base, "abc123");
        assert!(matches!(side, NewSide::WorkTree));

        let (base, side) = blame_targets("a1..b2", "a1..b2"); // exact range
        assert_eq!(base, "a1");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "b2"));
    }

    #[test]
    fn full_context_args_inserts_unified_context_after_diff_command() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(
            full_context_args(&args),
            vec!["diff", "--unified=2147483647", "main..HEAD"]
        );
    }

    #[test]
    fn name_only_args_inserts_the_flag_after_the_diff_command() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(
            name_only_args(&args),
            vec!["diff", "--name-only", "main..HEAD"]
        );
    }

    #[test]
    fn with_exclude_pathspecs_appends_literal_excludes_after_a_separator() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(
            with_exclude_pathspecs(&args, &["docs/plan.md".to_string(), "a[1].md".to_string()]),
            vec![
                "diff",
                "main..HEAD",
                "--",
                ":(exclude,literal)docs/plan.md",
                ":(exclude,literal)a[1].md",
            ]
        );
    }

    #[test]
    fn with_exclude_pathspecs_passes_args_through_when_nothing_is_hidden() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(with_exclude_pathspecs(&args, &[]), args);
    }
}
