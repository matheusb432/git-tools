use gtl_models::{
    diffs::{Commit, DiffLineCount, ExcludedExtensions},
    git::{GitDiffSpec, GitRange},
    paths::{RepositoryRelativePath, RepositoryRoot},
};
use gtl_parser::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};

use crate::{
    diffs::{FileDiff, FileStatus},
    ports::{GitClient, GitDiffFormat, GitDiffRequest},
};

fn parse_diff(raw: &str) -> anyhow::Result<Vec<FileDiff>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut line_classifier = UnifiedDiffLineClassifier::default();

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git a/")
            && let Some((_, path)) = rest.split_once(" b/")
        {
            files.extend(cur.take());
            line_classifier = UnifiedDiffLineClassifier::default();
            cur = Some(FileDiff {
                path: RepositoryRelativePath::try_new(path.into())?,
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
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
            UnifiedDiffLineKind::Added => file.added.increment(),
            UnifiedDiffLineKind::Removed => file.removed.increment(),
            UnifiedDiffLineKind::Meta
            | UnifiedDiffLineKind::Hunk { .. }
            | UnifiedDiffLineKind::Context => {}
        }
    }

    if let Some(file) = cur {
        files.push(file);
    }

    Ok(files)
}

fn attach_full_context(files: &mut [FileDiff], full_files: Vec<FileDiff>) {
    let mut full_by_path: std::collections::HashMap<RepositoryRelativePath, Vec<String>> =
        full_files
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

pub(super) struct DiffData {
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub hidden_paths: Vec<RepositoryRelativePath>,
}

pub(super) fn assemble(
    source: &impl GitClient,
    repo_path: &RepositoryRoot,
    diff_spec: &GitDiffSpec,
    log_range: &GitRange,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<DiffData> {
    let commits = source.log_commits(repo_path, log_range)?;

    let hidden_paths = hidden_paths(source, repo_path, diff_spec, excluded)?;
    let content_request = GitDiffRequest {
        spec: diff_spec.clone(),
        format: GitDiffFormat::Unified,
        excluded_paths: hidden_paths.clone(),
    };
    // Enforce exclusions even when a source ignores pathspecs.
    let (mut files, _) = filter_excluded_files(
        parse_diff(&source.diff(repo_path, &content_request)?)?,
        excluded,
    );
    let full_context_request = GitDiffRequest {
        format: GitDiffFormat::FullContext,
        ..content_request
    };
    attach_full_context(
        &mut files,
        parse_diff(&source.diff(repo_path, &full_context_request)?)?,
    );
    Ok(DiffData {
        commits,
        files,
        hidden_paths,
    })
}

fn filter_excluded_files(
    files: Vec<FileDiff>,
    excluded: &ExcludedExtensions,
) -> (Vec<FileDiff>, Vec<RepositoryRelativePath>) {
    if excluded.is_empty() {
        return (files, Vec::new());
    }
    let (hidden, kept): (Vec<FileDiff>, Vec<FileDiff>) = files
        .into_iter()
        .partition(|file| excluded.matches(&file.path));
    (kept, hidden.into_iter().map(|file| file.path).collect())
}

fn hidden_paths(
    source: &impl GitClient,
    repo_path: &RepositoryRoot,
    spec: &GitDiffSpec,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<Vec<RepositoryRelativePath>> {
    if excluded.is_empty() {
        return Ok(Vec::new());
    }
    Ok(source
        .diff(
            repo_path,
            &GitDiffRequest {
                spec: spec.clone(),
                format: GitDiffFormat::NamesOnly,
                excluded_paths: Vec::new(),
            },
        )?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| RepositoryRelativePath::try_new(path.into()).map_err(anyhow::Error::from))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| excluded.matches(path))
        .collect())
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
        let files = parse_diff(SAMPLE).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path.to_string_lossy(), "f.txt");
        assert_eq!(files[0].added, DiffLineCount::new(2));
        assert_eq!(files[0].removed, DiffLineCount::new(1));
        assert_eq!(files[1].path.to_string_lossy(), "g.txt");
        assert_eq!(files[1].added, DiffLineCount::new(1));
        assert_eq!(files[1].removed, DiffLineCount::default());
    }

    #[test]
    fn parse_diff_does_not_count_file_headers_as_changes() {
        let files = parse_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n").unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, DiffLineCount::default());
        assert_eq!(files[0].removed, DiffLineCount::default());
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
        )
        .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].removed, DiffLineCount::new(1));
        assert_eq!(files[0].added, DiffLineCount::new(2));
    }

    #[test]
    fn parse_diff_returns_empty_for_blank_input() {
        assert!(parse_diff("").unwrap().is_empty());
        assert!(parse_diff("   \n\t").unwrap().is_empty());
    }

    #[test]
    fn attach_full_context_only_sets_modified_files_with_extra_context() {
        let mut files = parse_diff(SAMPLE).unwrap();
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
        )
        .unwrap();

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
    fn parse_diff_rejects_parent_traversing_file_headers() {
        assert!(parse_diff("diff --git a/../secret b/../secret\n").is_err());
    }
}
