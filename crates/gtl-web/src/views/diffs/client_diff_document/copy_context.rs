use std::fmt;

use gtl_models::settings::ViewerLanguage;

use crate::shared::i18n::t;

mod selection;
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

    fn format(
        self,
        path: &str,
        comment_leader: &str,
        format: CopyFormat,
    ) -> Option<SelectedDiffCopy> {
        if self.lines.is_empty() {
            return None;
        }

        let source = self.lines.join("\n");
        if matches!(format, CopyFormat::Plain) {
            return Some(SelectedDiffCopy {
                text: source,
                status: SelectionCopyStatus::Plain,
            });
        }

        let line_range = self
            .line_range
            .map(|range| format!(", lines: {range}"))
            .unwrap_or_default();
        Some(SelectedDiffCopy {
            text: format!("{comment_leader} * {path}{line_range}\n{source}"),
            status: SelectionCopyStatus::File(self.line_range),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyFormat {
    Plain,
    WithLineContext,
}

impl CopyFormat {
    const fn from_setting(enabled: bool) -> Self {
        if enabled {
            Self::WithLineContext
        } else {
            Self::Plain
        }
    }
}

/// Selected source and the message to show after copying it.
#[derive(Debug, PartialEq, Eq)]
struct SelectedDiffCopy {
    text: String,
    status: SelectionCopyStatus,
}

/// What a source copy reports once the clipboard holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionCopyStatus {
    Plain,
    /// One file, with its line range when every copied line is numbered.
    File(Option<SelectedLineRange>),
    /// Several files.
    Files(usize),
}

impl SelectionCopyStatus {
    fn message(self, language: ViewerLanguage) -> String {
        match self {
            Self::Plain => t!(language, "copy-copied"),
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
            selected.format("src/main.rs", "//", CopyFormat::WithLineContext),
            Some(SelectedDiffCopy {
                text: "// * src/main.rs, lines: 12..13\nlet first = 1;\nlet second = 2;".to_owned(),
                status: SelectionCopyStatus::File(Some(SelectedLineRange {
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
            selected.format("scripts/run.sh", "#", CopyFormat::WithLineContext),
            Some(SelectedDiffCopy {
                text: "# * scripts/run.sh, lines: 7\necho ready".to_owned(),
                status: SelectionCopyStatus::File(Some(SelectedLineRange { first: 7, last: 7 })),
            })
        );
    }

    #[test]
    fn a_selection_without_source_rows_falls_through() {
        assert_eq!(
            SelectedDiffLines::default().format("src/main.rs", "//", CopyFormat::Plain),
            None
        );
    }

    #[test]
    fn plain_copy_contains_only_selected_source() {
        let mut selected = SelectedDiffLines::default();
        selected.push("let first = 1;".to_owned(), Some(12));
        selected.push("let second = 2;".to_owned(), Some(13));

        assert_eq!(
            selected.format("src/main.rs", "//", CopyFormat::Plain),
            Some(SelectedDiffCopy {
                text: "let first = 1;\nlet second = 2;".to_owned(),
                status: SelectionCopyStatus::Plain,
            })
        );
    }
}
