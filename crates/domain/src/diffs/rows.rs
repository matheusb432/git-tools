//! Structured diff rows: the line-level model derived from a file's raw
//! unified-diff text, consumed by every renderer (Maud HTML and the native
//! viewer) so hunk/gutter parsing exists in exactly one place.

use super::file::{FileDiff, LineOwners};

/// Char length (marker excluded) beyond which a line is "long" and is exempt
/// from intra-line diffing and wrap layout. Shared presentation rule for all
/// renderers.
pub const MAX_LINE_COLS: usize = 2000;

/// What a single diff row is: git metadata, a hunk header, or a
/// context/added/deleted code line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Meta,
    Hunk,
    Context,
    Add,
    Del,
}

/// One rendered diff row. `text` is the raw diff line including its leading
/// `+`/`-`/space marker; gutter numbers are absolute file line numbers and
/// present only on the side(s) the row exists on; `owner` is the short sha of
/// the commit owning a changed line (never set on context/meta/hunk rows).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub kind: RowKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
    pub owner: Option<String>,
}

/// True for git's non-code diff lines (file headers, mode/rename/similarity
/// markers, binary notices, the `\ No newline` marker).
pub fn is_meta_line(raw: &str) -> bool {
    raw.starts_with("index ")
        || raw.starts_with("--- ")
        || raw.starts_with("+++ ")
        || raw.starts_with("new file")
        || raw.starts_with("deleted file")
        || raw.starts_with("old mode")
        || raw.starts_with("new mode")
        || raw.starts_with("similarity ")
        || raw.starts_with("rename ")
        || raw.starts_with("Binary ")
        || raw.starts_with('\\')
}

/// Derive structured rows from a file's raw diff lines, tracking absolute
/// gutter numbers from hunk headers and attaching per-line commit ownership.
/// Empty lines are skipped; a malformed hunk header degrades to a context row
/// (matching the historical renderer behavior).
pub fn derive_rows(lines: &[String], owners: &LineOwners) -> Vec<Row> {
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut rows = Vec::with_capacity(lines.len());

    for raw in lines {
        if raw.is_empty() {
            continue;
        }

        if is_meta_line(raw) {
            rows.push(Row {
                kind: RowKind::Meta,
                old_no: None,
                new_no: None,
                text: raw.clone(),
                owner: None,
            });
            continue;
        }

        if let Some((old_start, new_start)) = hunk_starts(raw) {
            old_no = old_start;
            new_no = new_start;
            rows.push(Row {
                kind: RowKind::Hunk,
                old_no: None,
                new_no: None,
                text: raw.clone(),
                owner: None,
            });
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            rows.push(Row {
                kind: RowKind::Add,
                old_no: None,
                new_no: Some(new_no),
                text: raw.clone(),
                owner: owners.added.get(&new_no).cloned(),
            });
            new_no += 1;
        } else if raw.starts_with('-') && !raw.starts_with("---") {
            rows.push(Row {
                kind: RowKind::Del,
                old_no: Some(old_no),
                new_no: None,
                text: raw.clone(),
                owner: owners.deleted.get(&old_no).cloned(),
            });
            old_no += 1;
        } else {
            rows.push(Row {
                kind: RowKind::Context,
                old_no: Some(old_no),
                new_no: Some(new_no),
                text: raw.clone(),
                owner: None,
            });
            old_no += 1;
            new_no += 1;
        }
    }

    rows
}

/// Parse `@@ -a[,b] +c[,d] @@ …` into the two start line numbers.
fn hunk_starts(raw: &str) -> Option<(u32, u32)> {
    let rest = raw.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let (new_part, _) = rest.split_once(" @@")?;
    Some((parse_hunk_range(old_part)?, parse_hunk_range(new_part)?))
}

fn parse_hunk_range(s: &str) -> Option<u32> {
    let (start, len) = match s.split_once(',') {
        Some((start, len)) => (start, Some(len)),
        None => (s, None),
    };

    if start.is_empty()
        || !start.chars().all(|ch| ch.is_ascii_digit())
        || len.is_some_and(|value| value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }

    start.parse().ok()
}

impl FileDiff {
    /// Structured rows for the compact (default-context) diff.
    pub fn rows(&self) -> Vec<Row> {
        derive_rows(&self.lines, &self.owners)
    }

    /// Structured rows for the full-file diff, when it was computed.
    pub fn full_rows(&self) -> Option<Vec<Row>> {
        self.full_lines
            .as_ref()
            .map(|lines| derive_rows(lines, &self.owners))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffs::LineOwners;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn classifies_rows_and_tracks_gutter_numbers() {
        let rows = derive_rows(
            &lines(&[
                "index 111..222 100644",
                "@@ -3,2 +7,2 @@",
                " keep",
                "-old",
                "+new",
            ]),
            &LineOwners::default(),
        );

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
    fn attaches_owners_to_changed_rows_by_absolute_line_number() {
        let mut owners = LineOwners::default();
        owners.deleted.insert(4, "abc123def".into());
        owners.added.insert(8, "fed321cba".into());

        let rows = derive_rows(
            &lines(&["@@ -3,2 +7,2 @@", " keep", "-old", "+new"]),
            &owners,
        );

        assert_eq!(rows[1].owner, None, "context rows never carry an owner");
        assert_eq!(rows[2].owner.as_deref(), Some("abc123def"));
        assert_eq!(rows[3].owner.as_deref(), Some("fed321cba"));
    }

    #[test]
    fn skips_empty_lines_entirely() {
        let rows = derive_rows(&lines(&["@@ -1 +1 @@", "", " x"]), &LineOwners::default());
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn hunk_without_lengths_parses_starts() {
        let rows = derive_rows(&lines(&["@@ -3 +7 @@", " k"]), &LineOwners::default());
        assert_eq!(rows[0].kind, RowKind::Hunk);
        assert_eq!((rows[1].old_no, rows[1].new_no), (Some(3), Some(7)));
    }

    #[test]
    fn malformed_hunk_header_falls_through_to_context() {
        let rows = derive_rows(&lines(&["@@ garbage @@"]), &LineOwners::default());
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
            let rows = derive_rows(&lines(&[raw]), &LineOwners::default());
            assert_eq!(rows[0].kind, RowKind::Meta, "{raw} must be meta");
        }
    }

    #[test]
    fn filediff_rows_uses_compact_lines_and_full_rows_uses_full_lines() {
        let file = crate::diffs::FileDiff {
            path: "src/a.rs".into(),
            added: 1,
            removed: 0,
            lines: lines(&["@@ -1 +1 @@", "+x"]),
            full_lines: Some(lines(&["@@ -1 +1 @@", " ctx", "+x"])),
            commits: vec![],
            owners: LineOwners::default(),
        };
        assert_eq!(file.rows().len(), 2);
        assert_eq!(file.full_rows().unwrap().len(), 3);

        let no_full = crate::diffs::FileDiff {
            full_lines: None,
            ..file
        };
        assert!(no_full.full_rows().is_none());
    }
}
