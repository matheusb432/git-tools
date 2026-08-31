use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};
use gtl_parser::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};

use super::FileDiff;

pub(super) fn parse(raw: &str) -> anyhow::Result<Vec<FileDiff>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut line_classifier = UnifiedDiffLineClassifier::default();

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git a/")
            && let Some((_, path)) = rest.split_once(" b/")
        {
            files.extend(current.take());
            line_classifier = UnifiedDiffLineClassifier::default();
            current = Some(FileDiff {
                path: RepositoryRelativePath::try_new(path.into())?,
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: Vec::new(),
                full_lines: None,
            });
            continue;
        }

        let Some(file) = current.as_mut() else {
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

    if let Some(file) = current {
        files.push(file);
    }

    Ok(files)
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
    fn splits_two_file_diff_and_counts_body_changes() {
        let files = parse(SAMPLE).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path.to_string_lossy(), "f.txt");
        assert_eq!(files[0].added, DiffLineCount::new(2));
        assert_eq!(files[0].removed, DiffLineCount::new(1));
        assert_eq!(files[1].path.to_string_lossy(), "g.txt");
        assert_eq!(files[1].added, DiffLineCount::new(1));
        assert_eq!(files[1].removed, DiffLineCount::default());
    }

    #[test]
    fn does_not_count_file_headers_as_changes() {
        let files = parse("diff --git a/a b/a\n--- a/a\n+++ b/a\n").unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, DiffLineCount::default());
        assert_eq!(files[0].removed, DiffLineCount::default());
    }

    #[test]
    fn counts_header_like_hunk_content() {
        let files = parse(
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
    fn returns_empty_for_blank_input() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("   \n\t").unwrap().is_empty());
    }

    #[test]
    fn rejects_parent_traversing_file_headers() {
        assert!(parse("diff --git a/../secret b/../secret\n").is_err());
    }
}
