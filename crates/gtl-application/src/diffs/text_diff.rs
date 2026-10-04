//! Diff text supplied outside a repository, normalized to the shape Git capture produces.

use std::collections::HashSet;

use gtl_models::{
    diffs::{AppliedExtensionFilter, DiffText, DiffViewTitle, ExtensionFilter},
    failure::{DiffTextFailure, ExternalDiagnostic},
    paths::ProjectName,
};

use super::{
    Cmd, FileDiff, Foot, FullContextDiffState, TextOrigin, View, ViewOrigin,
    file_filter::DiffFileFilter, unified_diff,
};

const SIGNATURE_SEPARATOR: &str = "-- ";

/// Builds the view of `text`; `label` names it where a repository name would appear. Files the
/// filter hides stay in the view so a later filter change can show them again.
pub(crate) fn view(
    text: &DiffText,
    label: ProjectName,
    filter: &ExtensionFilter,
) -> Result<View, DiffTextFailure> {
    let (hidden_files, files): (Vec<_>, Vec<_>) = parse_files(text)?
        .into_iter()
        .partition(|file| filter.is_active() && filter.hides(&file.path));
    let extension_filter = AppliedExtensionFilter::from_hidden(
        filter,
        hidden_files.iter().map(|file| file.path.clone()).collect(),
    );
    Ok(View {
        file_filter: DiffFileFilter::text(filter.clone(), hidden_files),
        cmd: Cmd {
            lead: String::new(),
            range: label.to_string(),
            trail: String::new(),
        },
        foot: Foot {
            cmd: label.to_string(),
        },
        origin: ViewOrigin::Text(TextOrigin {
            label,
            id: text.id().clone(),
        }),
        commits: Vec::new(),
        files,
        title: DiffViewTitle::Diff,
        full_context: FullContextDiffState::Unavailable,
        extension_filter,
    })
}

/// Splits `text` into file diffs, ignoring text outside file sections and completed hunks.
pub(crate) fn parse_files(text: &DiffText) -> Result<Vec<FileDiff>, DiffTextFailure> {
    let normalized = normalize(text.as_str())?;
    let files =
        unified_diff::parse(&normalized).map_err(|error| DiffTextFailure::InvalidFileHeader {
            diagnostic: ExternalDiagnostic::new(&format!("{error:#}")),
        })?;
    if files.is_empty() {
        return Err(DiffTextFailure::NoFiles);
    }
    let mut paths = HashSet::with_capacity(files.len());
    for file in &files {
        if !paths.insert(&file.path) {
            return Err(DiffTextFailure::DuplicatePath {
                path: file.path.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(files)
}

enum Section {
    Outside,
    Header,
    Hunk { lines_old: u64, lines_new: u64 },
    AfterHunk,
}

fn normalize(text: &str) -> Result<String, DiffTextFailure> {
    let carriage_returns = text
        .split('\n')
        .find(|line| line.starts_with("diff --git "))
        .is_some_and(|line| line.ends_with('\r'));
    let mut normalized = String::with_capacity(text.len());
    let mut section = Section::Outside;
    for line in text.split('\n') {
        let line = if carriage_returns {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            line
        };
        if let Section::Hunk {
            lines_old,
            lines_new,
        } = section
        {
            if let Some((kept, lines_old, lines_new)) = hunk_line(line, lines_old, lines_new) {
                push_line(&mut normalized, kept);
                section = hunk_section(lines_old, lines_new);
                continue;
            }
            section = Section::AfterHunk;
        }
        if let Some(names) = line.strip_prefix("diff --git ") {
            validate_file_header(line, names)?;
            push_line(&mut normalized, line);
            section = Section::Header;
            continue;
        }
        match section {
            Section::Outside | Section::Hunk { .. } => {}
            Section::Header if line == SIGNATURE_SEPARATOR => section = Section::Outside,
            Section::Header | Section::AfterHunk => {
                if let Some((lines_old, lines_new)) = hunk_lengths(line) {
                    push_line(&mut normalized, line);
                    section = hunk_section(lines_old, lines_new);
                } else if matches!(section, Section::Header) || line.starts_with('\\') {
                    push_line(&mut normalized, line);
                }
            }
        }
    }
    normalized.truncate(normalized.trim_end_matches('\n').len());
    Ok(normalized)
}

const fn hunk_section(lines_old: u64, lines_new: u64) -> Section {
    if lines_old == 0 && lines_new == 0 {
        Section::AfterHunk
    } else {
        Section::Hunk {
            lines_old,
            lines_new,
        }
    }
}

fn push_line(normalized: &mut String, line: &str) {
    if !normalized.is_empty() {
        normalized.push('\n');
    }
    normalized.push_str(line);
}

fn validate_file_header(line: &str, names: &str) -> Result<(), DiffTextFailure> {
    match unified_diff::header_new_path(names) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(DiffTextFailure::InvalidFileHeader {
            diagnostic: ExternalDiagnostic::new(line),
        }),
        Err(error) => Err(DiffTextFailure::InvalidFileHeader {
            diagnostic: ExternalDiagnostic::new(&format!("{line}: {error:#}")),
        }),
    }
}

/// Consumes one hunk body line against the remaining old and new line counts. A blank line is an
/// empty context line whose leading space was stripped in transit.
fn hunk_line(line: &str, lines_old: u64, lines_new: u64) -> Option<(&str, u64, u64)> {
    match line.as_bytes().first() {
        None if lines_old > 0 && lines_new > 0 => Some((" ", lines_old - 1, lines_new - 1)),
        Some(b' ') if lines_old > 0 && lines_new > 0 => Some((line, lines_old - 1, lines_new - 1)),
        Some(b'-') if lines_old > 0 => Some((line, lines_old - 1, lines_new)),
        Some(b'+') if lines_new > 0 => Some((line, lines_old, lines_new - 1)),
        Some(b'\\') => Some((line, lines_old, lines_new)),
        _ => None,
    }
}

fn hunk_lengths(line: &str) -> Option<(u64, u64)> {
    let ranges = line.strip_prefix("@@ -")?;
    let (range_old, rest) = ranges.split_once(" +")?;
    let (range_new, _) = rest.split_once(" @@")?;
    Some((range_length(range_old)?, range_length(range_new)?))
}

fn range_length(range: &str) -> Option<u64> {
    let (start, length) = range.split_once(',').unwrap_or((range, "1"));
    start.parse::<u64>().ok()?;
    length.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> DiffText {
        DiffText::try_new(raw.to_owned()).unwrap()
    }

    fn paths(files: &[FileDiff]) -> Vec<String> {
        files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect()
    }

    const RENAMED_PLAN: &str = "\
diff --git a/plans/rollout.md b/plans/archived/rollout.md
similarity index 99%
rename from plans/rollout.md
rename to plans/archived/rollout.md
index 696dea98..17d50b8b 100644
--- a/plans/rollout.md
+++ b/plans/archived/rollout.md
@@ -1,3 +1,3 @@
 ---
-status: executing
+status: done
 date: 2026-07-16";

    #[test]
    fn review_package_preamble_is_ignored() {
        let package = format!(
            "# Review package: 1111111..2222222\n\n## Commits\n2222222 docs: archive\n\n## Diff\n{RENAMED_PLAN}\n"
        );

        let files = parse_files(&text(&package)).unwrap();

        assert_eq!(paths(&files), ["plans/archived/rollout.md"]);
        assert_eq!((files[0].added.value(), files[0].removed.value()), (1, 1));
        assert_eq!(
            files[0].lines,
            parse_files(&text(RENAMED_PLAN)).unwrap()[0].lines
        );
    }

    #[test]
    fn text_after_a_completed_hunk_is_dropped() {
        let patch = format!(
            "{RENAMED_PLAN}\n-- \n2.43.0\n\nFrom 3333333 Mon Sep 17 00:00:00 2001\nSubject: [PATCH] next\n"
        );

        let files = parse_files(&text(&patch)).unwrap();

        assert_eq!(
            files[0].lines,
            parse_files(&text(RENAMED_PLAN)).unwrap()[0].lines
        );
    }

    #[test]
    fn signature_after_a_header_only_file_is_dropped() {
        let patch = "diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n-- \n2.43.0\n";

        let files = parse_files(&text(patch)).unwrap();

        assert_eq!(
            files[0].lines,
            ["old mode 100644", "new mode 100755"].into_iter().collect()
        );
    }

    #[test]
    fn removed_signature_like_line_inside_a_hunk_is_kept() {
        let patch = "diff --git a/notes.md b/notes.md\n--- a/notes.md\n+++ b/notes.md\n@@ -1,2 +1 @@\n-- \n kept\n";

        let files = parse_files(&text(patch)).unwrap();

        assert_eq!((files[0].added.value(), files[0].removed.value()), (0, 1));
    }

    #[test]
    fn blank_hunk_lines_become_empty_context_lines() {
        let stripped = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,3 @@
 one

-three
+four";
        let canonical = stripped.replace("\n\n", "\n \n");

        assert_eq!(
            parse_files(&text(stripped)).unwrap()[0].lines,
            parse_files(&text(&canonical)).unwrap()[0].lines
        );
    }

    #[test]
    fn carriage_return_line_endings_are_removed() {
        let crlf = RENAMED_PLAN.replace('\n', "\r\n");

        assert_eq!(
            parse_files(&text(&crlf)).unwrap()[0].lines,
            parse_files(&text(RENAMED_PLAN)).unwrap()[0].lines
        );
    }

    #[test]
    fn text_without_file_sections_is_rejected() {
        assert_eq!(
            parse_files(&text("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n")),
            Err(DiffTextFailure::NoFiles)
        );
    }

    #[test]
    fn repeated_paths_are_rejected() {
        let twice = format!("{RENAMED_PLAN}\n{RENAMED_PLAN}");

        assert_eq!(
            parse_files(&text(&twice)),
            Err(DiffTextFailure::DuplicatePath {
                path: "plans/archived/rollout.md".to_owned()
            })
        );
    }

    #[test]
    fn headers_without_git_prefixes_are_rejected() {
        let error = parse_files(&text("diff --git x y\n")).unwrap_err();

        assert!(matches!(error, DiffTextFailure::InvalidFileHeader { .. }));
    }
}
