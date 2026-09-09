#![cfg(test)]

use gtl_application::diffs::{FileDiff, FileStatus};
use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};

fn file(path: &str) -> FileDiff {
    FileDiff {
        path: RepositoryRelativePath::try_new(path.into()).unwrap(),
        added: DiffLineCount::default(),
        removed: DiffLineCount::default(),
        lines: gtl_application::diffs::source_lines::DiffSourceLines::default(),
        full_lines: None,
    }
}

fn file_with_lines(lines: &[&str]) -> FileDiff {
    let mut file = file("example.rs");
    file.lines = lines.iter().map(|line| (*line).to_string()).collect();
    file
}

#[test]
fn file_status_classifies_raw_git_metadata() {
    for (lines, expected) in [
        (vec!["new file mode 100644"], FileStatus::Added),
        (vec!["+++ /dev/null"], FileStatus::Deleted),
        (vec!["rename from old.rs"], FileStatus::Renamed),
        (vec!["@@ -1 +1 @@"], FileStatus::Modified),
    ] {
        assert_eq!(file_with_lines(&lines).status(), expected);
    }
}
