//! Viewer-local IPC DTOs mapped from domain diff types (the
//! `HistoryRecord → HistoryEntry` pattern; `contracts` is untouched).
//!
//! Binding decision: row `text` crosses the IPC boundary WITHOUT its leading
//! `+`/`-`/space marker for context/add/del rows — `kind` carries the
//! semantics and the frontend re-adds presentation markers where needed.
//! Meta/hunk text passes raw (it has no marker). `long_len` is computed on the
//! raw line via the shared domain rule, so it matches the Maud renderer.

use domain::diffs::{
    Commit, FileDiff, Row, RowKind, Span, SplitCell, SplitRow, View, line_body, long_line_len,
};
use serde::Serialize;

/// One commit card for the shelf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitDto {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
    pub parents: Vec<String>,
    pub members: Vec<String>,
    pub is_merge: bool,
}

/// One file's summary line: everything the sidebar/file header needs without
/// fetching any rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileSummary {
    pub path: String,
    pub status: &'static str,
    pub added: u32,
    pub removed: u32,
    pub commits: Vec<String>,
    pub has_full: bool,
}

/// Everything a tab shell renders before any rows are fetched. Small even for
/// huge diffs (KBs): summaries, never lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabMeta {
    pub tab_id: u64,
    pub batch_id: String,
    pub title: String,
    pub repo_name: String,
    pub repo_root: String,
    pub branch: String,
    pub upstream: String,
    pub cmd_lead: String,
    pub cmd_range: String,
    pub cmd_trail: String,
    pub commits_label: String,
    pub foot_cmd: String,
    pub foot_note: String,
    pub is_empty: bool,
    pub commits: Vec<CommitDto>,
    pub files: Vec<FileSummary>,
}

/// One unified-pane row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowDto {
    pub kind: &'static str,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
    pub owner: Option<String>,
    pub long_len: Option<usize>,
}

/// A changed-char span within a stripped line body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpanDto {
    pub start: usize,
    pub end: usize,
}

/// One side of a split pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SplitCellDto {
    pub no: u32,
    pub text: String,
    pub owner: Option<String>,
    pub spans: Vec<SpanDto>,
    pub long_len: Option<usize>,
}

/// One split-pane row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SplitRowDto {
    Meta {
        text: String,
    },
    Hunk {
        text: String,
    },
    Context {
        old_no: u32,
        new_no: u32,
        text: String,
        long_len: Option<usize>,
    },
    Pair {
        old: Option<SplitCellDto>,
        new: Option<SplitCellDto>,
    },
}

pub(crate) fn tab_meta(tab_id: u64, batch_id: &str, view: &View) -> TabMeta {
    TabMeta {
        tab_id,
        batch_id: batch_id.to_string(),
        title: view.title.clone(),
        repo_name: view.repo_name.clone(),
        repo_root: view.repo_root.clone(),
        branch: view.branch.clone(),
        upstream: view.upstream.clone(),
        cmd_lead: view.cmd.lead.clone(),
        cmd_range: view.cmd.range.clone(),
        cmd_trail: view.cmd.trail.clone(),
        commits_label: view.commits_label.clone(),
        foot_cmd: view.foot.cmd.clone(),
        foot_note: view.foot.note.clone(),
        is_empty: view.is_empty(),
        commits: view.commits.iter().map(commit_dto).collect(),
        files: view.files.iter().map(file_summary).collect(),
    }
}

fn commit_dto(commit: &Commit) -> CommitDto {
    CommitDto {
        sha: commit.sha.clone(),
        subject: commit.subject.clone(),
        body: commit.body.clone(),
        date: commit.date.clone(),
        iso: commit.iso.clone(),
        parents: commit.parents.clone(),
        members: commit.members.clone(),
        is_merge: commit.is_merge(),
    }
}

fn file_summary(file: &FileDiff) -> FileSummary {
    FileSummary {
        path: file.path.clone(),
        status: file.status().key(),
        added: file.added,
        removed: file.removed,
        commits: file.commits.clone(),
        has_full: file.full_lines.is_some(),
    }
}

fn kind_key(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Meta => "meta",
        RowKind::Hunk => "hunk",
        RowKind::Context => "context",
        RowKind::Add => "add",
        RowKind::Del => "del",
    }
}

pub(crate) fn row_dto(row: &Row) -> RowDto {
    let long_len = long_line_len(&row.text);
    let strip = matches!(row.kind, RowKind::Context | RowKind::Add | RowKind::Del);
    RowDto {
        kind: kind_key(row.kind),
        old_no: row.old_no,
        new_no: row.new_no,
        text: if strip {
            line_body(&row.text).to_string()
        } else {
            row.text.clone()
        },
        owner: row.owner.clone(),
        long_len,
    }
}

fn span_dto(span: &Span) -> SpanDto {
    SpanDto {
        start: span.start,
        end: span.end,
    }
}

fn split_cell_dto(cell: &SplitCell) -> SplitCellDto {
    SplitCellDto {
        no: cell.no,
        text: line_body(&cell.text).to_string(),
        owner: cell.owner.clone(),
        spans: cell.spans.iter().map(span_dto).collect(),
        long_len: long_line_len(&cell.text),
    }
}

pub(crate) fn split_row_dto(row: &SplitRow) -> SplitRowDto {
    match row {
        SplitRow::Meta { text } => SplitRowDto::Meta { text: text.clone() },
        SplitRow::Hunk { text } => SplitRowDto::Hunk { text: text.clone() },
        SplitRow::Context {
            old_no,
            new_no,
            text,
        } => SplitRowDto::Context {
            old_no: *old_no,
            new_no: *new_no,
            text: line_body(text).to_string(),
            long_len: long_line_len(text),
        },
        SplitRow::Pair { old, new } => SplitRowDto::Pair {
            old: old.as_ref().map(split_cell_dto),
            new: new.as_ref().map(split_cell_dto),
        },
    }
}

#[cfg(test)]
mod tests {
    use domain::diffs::{
        Cmd, Commit, FileDiff, Foot, LineOwners, MAX_LINE_COLS, View, derive_rows, split_rows,
    };

    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn row_text_crosses_without_the_marker_and_kind_carries_semantics() {
        let rows = derive_rows(
            &lines(&["@@ -1,2 +1,2 @@", " keep", "-old", "+new"]),
            &LineOwners::default(),
        );
        let dtos: Vec<RowDto> = rows.iter().map(row_dto).collect();

        assert_eq!(
            (dtos[0].kind, dtos[0].text.as_str()),
            ("hunk", "@@ -1,2 +1,2 @@")
        );
        assert_eq!((dtos[1].kind, dtos[1].text.as_str()), ("context", "keep"));
        assert_eq!((dtos[2].kind, dtos[2].text.as_str()), ("del", "old"));
        assert_eq!((dtos[3].kind, dtos[3].text.as_str()), ("add", "new"));
    }

    #[test]
    fn meta_rows_pass_raw_text() {
        let rows = derive_rows(&lines(&["index 111..222 100644"]), &LineOwners::default());
        let dto = row_dto(&rows[0]);
        assert_eq!(
            (dto.kind, dto.text.as_str()),
            ("meta", "index 111..222 100644")
        );
    }

    #[test]
    fn long_rows_carry_their_char_count() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let rows = derive_rows(&lines(&["@@ -1 +1 @@", &long]), &LineOwners::default());
        let dto = row_dto(&rows[1]);
        assert_eq!(dto.long_len, Some(MAX_LINE_COLS + 5));
        assert_eq!(
            dto.text.chars().count(),
            MAX_LINE_COLS + 5,
            "stripped, not truncated"
        );
    }

    #[test]
    fn split_pair_cells_strip_markers_and_keep_spans() {
        let rows = split_rows(&derive_rows(
            &lines(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]),
            &LineOwners::default(),
        ));
        let SplitRowDto::Pair {
            old: Some(old),
            new: Some(new),
        } = split_row_dto(&rows[1])
        else {
            panic!("pair expected");
        };
        assert_eq!(old.text, "let x = 1;");
        assert_eq!(new.text, "let x = 2;");
        assert_eq!(old.spans, vec![SpanDto { start: 8, end: 9 }]);
        assert_eq!(new.spans, vec![SpanDto { start: 8, end: 9 }]);
    }

    #[test]
    fn tab_meta_summarizes_without_any_lines() {
        let view = View {
            repo_name: "gt".into(),
            repo_root: "/repos/gt".into(),
            branch: "feature".into(),
            upstream: "origin/main".into(),
            commits: vec![Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                parents: vec!["p1".into(), "p2".into()],
                ..Default::default()
            }],
            files: vec![FileDiff {
                path: "src/a.rs".into(),
                added: 2,
                removed: 1,
                lines: lines(&["@@ -1,2 +1,3 @@", " k", "-o", "+n", "+e"]),
                full_lines: Some(lines(&["@@ -1 +1 @@", " k"])),
                commits: vec!["abc1234".into()],
                owners: LineOwners::default(),
            }],
            title: "diff".into(),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "origin/main..HEAD".into(),
                trail: String::new(),
            },
            commits_label: "commits".into(),
            foot: Foot {
                cmd: "gtl diff".into(),
                note: "note".into(),
            },
            theme: None,
        };

        let meta = tab_meta(7, "batch-1", &view);

        assert_eq!(meta.tab_id, 7);
        assert_eq!(meta.batch_id, "batch-1");
        assert_eq!(meta.cmd_range, "origin/main..HEAD");
        assert!(!meta.is_empty);
        assert!(meta.commits[0].is_merge);
        let file = &meta.files[0];
        assert_eq!(
            (
                file.path.as_str(),
                file.status,
                file.added,
                file.removed,
                file.has_full
            ),
            ("src/a.rs", "modified", 2, 1, true)
        );
    }
}
