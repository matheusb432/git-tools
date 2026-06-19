use std::collections::HashMap;

use crate::model::{FileDiff, FileStatus};

pub fn parse_diff(raw: &str) -> Vec<FileDiff> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let mut cur: Option<FileDiff> = None;

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git a/")
            && let Some((_, path)) = rest.split_once(" b/")
        {
            if let Some(file) = cur.take() {
                files.push(file);
            }
            cur = Some(FileDiff {
                path: path.to_string(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
                commits: Vec::new(),
            });
            continue;
        }

        let Some(file) = cur.as_mut() else {
            continue;
        };

        file.lines.push(line.to_string());
        if line.starts_with('+') && !line.starts_with("+++") {
            file.added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            file.removed += 1;
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
    fn full_context_args_inserts_unified_context_after_diff_command() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(
            full_context_args(&args),
            vec!["diff", "--unified=2147483647", "main..HEAD"]
        );
    }
}
