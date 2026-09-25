use std::fmt;

use gtl_models::settings::ViewerLanguage;

use crate::shared::i18n::t;

#[cfg(feature = "desktop")]
mod selection;
#[cfg(feature = "desktop")]
pub(super) use selection::use_diff_copy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedLineRange {
    first: u32,
    last: u32,
}

impl fmt::Display for SelectedLineRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.first == self.last {
            write!(formatter, "{}", self.first)
        } else {
            write!(formatter, "{}..{}", self.first, self.last)
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SelectedDiffLines {
    lines: Vec<String>,
    line_range: Option<SelectedLineRange>,
}

impl SelectedDiffLines {
    fn push(&mut self, line: String, line_number: Option<u32>) {
        self.lines.push(line);
        if let Some(line_number) = line_number {
            self.line_range = Some(match self.line_range {
                Some(range) => SelectedLineRange {
                    first: range.first,
                    last: line_number,
                },
                None => SelectedLineRange {
                    first: line_number,
                    last: line_number,
                },
            });
        }
    }

    fn with_context(self, path: &str, comment_leader: &str) -> Option<ContextualizedCopy> {
        if self.lines.is_empty() {
            return None;
        }

        let line_range = self
            .line_range
            .map(|range| format!(", lines: {range}"))
            .unwrap_or_default();
        Some(ContextualizedCopy {
            text: format!(
                "{comment_leader} * {path}{line_range}\n{}",
                self.lines.join("\n")
            ),
            status: ContextCopyStatus::File(self.line_range),
        })
    }
}

/// Copied source under a file-context header, and what the copy reports.
///
/// The header keeps one format in every display language, so tools that read
/// pasted context do not depend on the viewer's language.
#[derive(Debug, PartialEq, Eq)]
struct ContextualizedCopy {
    text: String,
    status: ContextCopyStatus,
}

/// What a context copy reports once the clipboard holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextCopyStatus {
    /// One file, with its line range when every copied line is numbered.
    File(Option<SelectedLineRange>),
    /// Several files.
    Files(usize),
}

impl ContextCopyStatus {
    #[cfg_attr(
        not(feature = "desktop"),
        expect(dead_code, reason = "offline artifacts copy through their enhancer")
    )]
    fn message(self, language: ViewerLanguage) -> String {
        match self {
            Self::File(None) => t!(language, "copy-context"),
            Self::File(Some(range)) => {
                t!(language, "copy-context-lines", lines = range.to_string())
            }
            Self::Files(count) => t!(language, "copy-context-files", count = count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_lines_copy_as_clean_source_under_their_file_context() {
        let mut selected = SelectedDiffLines::default();
        selected.push("let first = 1;".to_owned(), Some(12));
        selected.push("let second = 2;".to_owned(), Some(13));

        assert_eq!(
            selected.with_context("src/main.rs", "//"),
            Some(ContextualizedCopy {
                text: "// * src/main.rs, lines: 12..13\nlet first = 1;\nlet second = 2;".to_owned(),
                status: ContextCopyStatus::File(Some(SelectedLineRange {
                    first: 12,
                    last: 13
                })),
            })
        );
    }

    #[test]
    fn a_single_selected_line_uses_one_line_number() {
        let mut selected = SelectedDiffLines::default();
        selected.push("echo ready".to_owned(), Some(7));

        assert_eq!(
            selected.with_context("scripts/run.sh", "#"),
            Some(ContextualizedCopy {
                text: "# * scripts/run.sh, lines: 7\necho ready".to_owned(),
                status: ContextCopyStatus::File(Some(SelectedLineRange { first: 7, last: 7 })),
            })
        );
    }

    #[test]
    fn a_selection_without_source_rows_falls_through() {
        assert_eq!(
            SelectedDiffLines::default().with_context("src/main.rs", "//"),
            None
        );
    }
}
