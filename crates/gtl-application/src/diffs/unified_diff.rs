use anyhow::Context as _;
use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};
use gtl_parser::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};

use super::{FileDiff, source_lines::DiffSourceLines};

pub(super) fn parse(raw: &str) -> anyhow::Result<Vec<FileDiff>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut current: Option<(FileDiff, Vec<&str>)> = None;
    let mut line_classifier = UnifiedDiffLineClassifier::default();

    for line in raw.split('\n') {
        if let Some(names) = line.strip_prefix("diff --git ")
            && let Some(path) = header_new_path(names)?
        {
            files.extend(current.take().map(finish_file));
            line_classifier = UnifiedDiffLineClassifier::default();
            current = Some((
                FileDiff {
                    path: RepositoryRelativePath::try_new(path.into())?,
                    added: DiffLineCount::default(),
                    removed: DiffLineCount::default(),
                    lines: DiffSourceLines::default(),
                    full_lines: None,
                },
                Vec::new(),
            ));
            continue;
        }

        let Some((file, lines)) = current.as_mut() else {
            continue;
        };

        // Hunk lines start with a marker, so these can only be extended headers. They name a
        // rename or copy target exactly, where the `diff --git` names can split ambiguously.
        if let Some(new_path) = line
            .strip_prefix("rename to ")
            .or_else(|| line.strip_prefix("copy to "))
        {
            file.path = RepositoryRelativePath::try_new(unquote_git_path(new_path)?.into())?;
        }
        lines.push(line);
        match line_classifier.classify(line) {
            UnifiedDiffLineKind::Added => file.added.increment(),
            UnifiedDiffLineKind::Removed => file.removed.increment(),
            UnifiedDiffLineKind::Meta
            | UnifiedDiffLineKind::Hunk { .. }
            | UnifiedDiffLineKind::Context => {}
        }
    }

    if let Some(file) = current {
        files.push(finish_file(file));
    }

    Ok(files)
}

/// Reads the new-side path from the names after `diff --git `.
pub(super) fn header_new_path(names: &str) -> anyhow::Result<Option<String>> {
    let new_side = if names.ends_with('"') {
        // A quoted name escapes its own quotes, so ` "b/` only starts the new-side name.
        let Some(start) = names.rfind(" \"b/") else {
            return Ok(None);
        };
        unquote_git_path(&names[start + 1..])?
    } else if names.starts_with('"') {
        // An unquoted name holds no quote, so the last quote closes the old-side name.
        let Some((_, new_side)) = names.rsplit_once("\" ") else {
            return Ok(None);
        };
        new_side.to_owned()
    } else {
        let Some(separator) = same_name_separator(names).or_else(|| names.find(" b/")) else {
            return Ok(None);
        };
        names[separator + 1..].to_owned()
    };
    Ok(new_side.strip_prefix("b/").map(str::to_owned))
}

/// Finds the ` b/` separator between two identical unquoted names, which Git prints for every
/// change except a rename or copy. Git leaves spaces unquoted, so `a/x b/y b/x b/y` only splits
/// unambiguously where both sides match.
fn same_name_separator(names: &str) -> Option<usize> {
    let separator = names.len() / 2;
    let old_name = names.get(..separator)?.strip_prefix("a/")?;
    let new_name = names.get(separator..)?.strip_prefix(" b/")?;
    (old_name == new_name).then_some(separator)
}

/// Decodes a path Git printed in C-style quotes, which it uses for names holding a quote,
/// backslash, control character, or non-ASCII byte; other paths are returned unchanged.
pub(super) fn unquote_git_path(raw: &str) -> anyhow::Result<String> {
    let Some(quoted) = raw
        .strip_prefix('"')
        .and_then(|quoted| quoted.strip_suffix('"'))
    else {
        return Ok(raw.to_owned());
    };
    let mut bytes = Vec::with_capacity(quoted.len());
    let mut remaining = quoted.bytes();
    while let Some(byte) = remaining.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let escaped = remaining
            .next()
            .with_context(|| format!("Git path {raw} ends inside an escape"))?;
        bytes.push(match escaped {
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            b'0'..=b'3' => {
                let mut value = escaped - b'0';
                for _ in 0..2 {
                    let digit = remaining
                        .next()
                        .filter(|digit| (b'0'..=b'7').contains(digit))
                        .with_context(|| format!("Git path {raw} has a short octal escape"))?;
                    value = value * 8 + (digit - b'0');
                }
                value
            }
            other => other,
        });
    }
    String::from_utf8(bytes).with_context(|| format!("Git path {raw} is not valid UTF-8"))
}

fn finish_file((mut file, lines): (FileDiff, Vec<&str>)) -> FileDiff {
    file.lines = lines.into_iter().collect();
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffs::FileStatus;

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
        assert_eq!(
            parse("").unwrap(),
            Vec::<crate::diffs::file::FileDiff>::new()
        );
        assert_eq!(
            parse("   \n\t").unwrap(),
            Vec::<crate::diffs::file::FileDiff>::new()
        );
    }

    #[test]
    fn decodes_quoted_header_paths() {
        let files = parse(concat!(
            "diff --git \"a/caf\\303\\251.rs\" \"b/caf\\303\\251.rs\"\n",
            "diff --git a/old.txt \"b/tab\\there \\\"q\\\" \\\\.txt\"\n",
            "diff --git \"a/say \\\"hi\\\" b/x\" b/plain b/name.txt\n",
        ))
        .unwrap();

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["café.rs", "tab\there \"q\" \\.txt", "plain b/name.txt",]
        );
    }

    #[test]
    fn splits_unquoted_names_with_spaces_where_both_sides_match() {
        let files = parse("diff --git a/dir b/file.txt b/dir b/file.txt\n").unwrap();

        assert_eq!(files[0].path.to_string_lossy(), "dir b/file.txt");
    }

    #[test]
    fn takes_rename_and_copy_targets_from_their_extended_headers() {
        let files = parse(concat!(
            "diff --git a/old b/name.txt b/new name.txt\n",
            "similarity index 100%\n",
            "rename from old b/name.txt\n",
            "rename to new name.txt\n",
            "diff --git a/source.rs \"b/caf\\303\\251.rs\"\n",
            "similarity index 100%\n",
            "copy from source.rs\n",
            "copy to \"caf\\303\\251.rs\"\n",
        ))
        .unwrap();

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["new name.txt", "café.rs"]
        );
        assert_eq!(files[0].status(), FileStatus::Renamed);
    }

    #[test]
    fn unquotes_only_c_quoted_git_paths() {
        assert_eq!(unquote_git_path("plain name.rs").unwrap(), "plain name.rs");
        assert_eq!(
            unquote_git_path(r#""na\303\257ve.md""#).unwrap(),
            "naïve.md"
        );
        assert!(unquote_git_path(r#""broken\30""#).is_err());
        assert!(unquote_git_path(r#""invalid\377""#).is_err());
    }

    #[test]
    fn rejects_parent_traversing_file_headers() {
        assert!(parse("diff --git a/../secret b/../secret\n").is_err());
    }
}
