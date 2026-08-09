//! Structured diff rows derived from classified unified-diff text.

use gtl_application::diffs::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};

/// Char length (marker excluded) beyond which a line is "long" and is exempt
/// from intra-line diffing and wrap layout. Shared presentation rule for all
/// renderers.
pub(super) const MAX_LINE_COLS: usize = 2000;

/// What a single diff row is: git metadata, a hunk header, or a
/// context/added/deleted code line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowKind {
    Meta,
    Hunk,
    Context,
    Add,
    Del,
}

/// One rendered diff row. `text` is the raw diff line including its leading
/// `+`/`-`/space marker; gutter numbers are absolute file line numbers and
/// present only on the side(s) the row exists on.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Row {
    pub(super) kind: RowKind,
    pub(super) old_no: Option<u32>,
    pub(super) new_no: Option<u32>,
    pub(super) text: String,
}

#[derive(Debug, Default)]
struct LineNumberState {
    old: u32,
    new: u32,
}

impl LineNumberState {
    fn advance(&mut self, line_kind: UnifiedDiffLineKind) -> (RowKind, Option<u32>, Option<u32>) {
        match line_kind {
            UnifiedDiffLineKind::Meta => (RowKind::Meta, None, None),
            UnifiedDiffLineKind::Hunk {
                line_number_old,
                line_number_new,
            } => {
                self.old = line_number_old;
                self.new = line_number_new;
                (RowKind::Hunk, None, None)
            }
            UnifiedDiffLineKind::Added => {
                let new = self.new;
                self.new += 1;
                (RowKind::Add, None, Some(new))
            }
            UnifiedDiffLineKind::Removed => {
                let old = self.old;
                self.old += 1;
                (RowKind::Del, Some(old), None)
            }
            UnifiedDiffLineKind::Context => {
                let old = self.old;
                let new = self.new;
                self.old += 1;
                self.new += 1;
                (RowKind::Context, Some(old), Some(new))
            }
        }
    }
}

/// Derive structured rows from a file's raw diff lines, tracking absolute
/// gutter numbers from hunk headers.
/// Empty lines are skipped; a malformed hunk header degrades to a context row
/// (matching the historical renderer behavior).
pub(super) fn derive_rows(lines: &[String]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(lines.len());
    let mut line_classifier = UnifiedDiffLineClassifier::default();
    let mut line_numbers = LineNumberState::default();

    for raw in lines {
        if raw.is_empty() {
            continue;
        }

        let (kind, old_no, new_no) = line_numbers.advance(line_classifier.classify(raw));
        rows.push(Row {
            kind,
            old_no,
            new_no,
            text: raw.clone(),
        });
    }

    rows
}

pub(super) fn line_number_digits(lines: &[String]) -> u32 {
    let mut line_classifier = UnifiedDiffLineClassifier::default();
    let mut line_numbers = LineNumberState::default();
    let mut line_number_max = 0;

    for raw in lines.iter().filter(|line| !line.is_empty()) {
        let (_, old_no, new_no) = line_numbers.advance(line_classifier.classify(raw));
        if let Some(line_number) = old_no {
            line_number_max = line_number_max.max(line_number);
        }
        if let Some(line_number) = new_no {
            line_number_max = line_number_max.max(line_number);
        }
    }

    line_number_max.checked_ilog10().unwrap_or(0) + 1
}

/// A changed line body with its leading diff marker stripped, ready for
/// intra-line diffing or marker-free presentation.
pub(super) fn line_body(raw: &str) -> &str {
    raw.get(1..).unwrap_or("")
}

/// The char count of `raw` (leading `+`/`-`/space marker excluded) when the
/// line is "long" (beyond [`MAX_LINE_COLS`]), else `None`. The single long-line
/// rule shared by every server-rendered row path: unified long-line taming, the
/// split pane's intra-line exemption, and the syntax-highlighting bypass.
pub(super) fn long_line_len(raw: &str) -> Option<usize> {
    let marker = usize::from(matches!(raw.as_bytes().first(), Some(b'+' | b'-' | b' ')));
    let len = raw.chars().count().saturating_sub(marker);
    (len > MAX_LINE_COLS).then_some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn classifies_rows_and_tracks_gutter_numbers() {
        let rows = derive_rows(&lines(&[
            "index 111..222 100644",
            "@@ -3,2 +7,2 @@",
            " keep",
            "-old",
            "+new",
        ]));

        assert_eq!(rows[0].kind, RowKind::Meta);
        assert_eq!((rows[0].old_no, rows[0].new_no), (None, None));
        assert_eq!(rows[1].kind, RowKind::Hunk);
        assert_eq!(rows[1].text, "@@ -3,2 +7,2 @@");
        assert_eq!(rows[2].kind, RowKind::Context);
        assert_eq!((rows[2].old_no, rows[2].new_no), (Some(3), Some(7)));
        assert_eq!(rows[3].kind, RowKind::Del);
        assert_eq!((rows[3].old_no, rows[3].new_no), (Some(4), None));
        assert_eq!(rows[4].kind, RowKind::Add);
        assert_eq!((rows[4].old_no, rows[4].new_no), (None, Some(8)));
        assert_eq!(rows[4].text, "+new");
    }

    #[test]
    fn header_like_hunk_content_keeps_changed_rows_and_gutters() {
        let rows = derive_rows(&lines(&[
            "--- a/a.sql",
            "+++ b/a.sql",
            "@@ -1,2 +1,3 @@",
            "--- old heading",
            "+-- new heading",
            "+++ literal",
            " keep",
        ]));

        assert_eq!(rows[0].kind, RowKind::Meta);
        assert_eq!(rows[1].kind, RowKind::Meta);
        assert_eq!(rows[2].kind, RowKind::Hunk);
        assert_eq!(
            (rows[3].kind, rows[3].old_no, rows[3].new_no),
            (RowKind::Del, Some(1), None)
        );
        assert_eq!(
            (rows[4].kind, rows[4].old_no, rows[4].new_no),
            (RowKind::Add, None, Some(1))
        );
        assert_eq!(
            (rows[5].kind, rows[5].old_no, rows[5].new_no),
            (RowKind::Add, None, Some(2))
        );
        assert_eq!(
            (rows[6].kind, rows[6].old_no, rows[6].new_no),
            (RowKind::Context, Some(2), Some(3))
        );
    }

    #[test]
    fn skips_empty_lines_entirely() {
        let rows = derive_rows(&lines(&["@@ -1 +1 @@", "", " x"]));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn hunk_without_lengths_parses_starts() {
        let rows = derive_rows(&lines(&["@@ -3 +7 @@", " k"]));
        assert_eq!(rows[0].kind, RowKind::Hunk);
        assert_eq!((rows[1].old_no, rows[1].new_no), (Some(3), Some(7)));
    }

    #[test]
    fn malformed_hunk_header_falls_through_to_context() {
        let rows = derive_rows(&lines(&["@@ garbage @@"]));
        assert_eq!(rows[0].kind, RowKind::Context);
        assert_eq!((rows[0].old_no, rows[0].new_no), (Some(0), Some(0)));
    }

    #[test]
    fn file_markers_no_newline_and_binary_are_meta() {
        for raw in [
            "--- a/x",
            "+++ b/x",
            "\\ No newline at end of file",
            "Binary files differ",
            "rename from a",
            "new file mode 100644",
            "deleted file mode 100644",
            "old mode 100644",
            "new mode 100755",
            "similarity index 90%",
            "index 111..222",
        ] {
            let rows = derive_rows(&lines(&[raw]));
            assert_eq!(rows[0].kind, RowKind::Meta, "{raw} must be meta");
        }
    }

    #[test]
    fn long_line_len_excludes_the_leading_marker() {
        let at_limit = format!("+{}", "a".repeat(MAX_LINE_COLS));
        assert_eq!(
            long_line_len(&at_limit),
            None,
            "exactly MAX cols is not long"
        );
        let over = format!("+{}", "a".repeat(MAX_LINE_COLS + 1));
        assert_eq!(long_line_len(&over), Some(MAX_LINE_COLS + 1));
    }

    #[test]
    fn long_line_len_counts_all_chars_without_a_marker() {
        let raw = "x".repeat(MAX_LINE_COLS + 1);
        assert_eq!(long_line_len(&raw), Some(MAX_LINE_COLS + 1));
    }

    #[test]
    fn line_body_strips_the_marker() {
        assert_eq!(line_body("-old"), "old");
        assert_eq!(line_body(""), "");
    }
}
