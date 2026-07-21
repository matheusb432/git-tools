//! Side-by-side presentation pairing of structured diff rows: within a hunk a
//! run of deletions pairs index-wise with the following run of additions
//! (shorter side padded), context mirrors on both panes, and paired short
//! lines carry intra-line changed-char spans.

use super::{
    intraline::{LineSpans, Span, changed_spans},
    model::{Row, RowKind, line_body, long_line_len},
};

pub(super) const PRESENTATION_CLASSES: &str = concat!(
    "[&_.diff-split_.dl]:grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] [&_.diff-split_.dl]:items-stretch ",
    "[&_.diff-split_:is(.dl-meta,.dl-hunk)]:grid-cols-[minmax(0,1fr)] ",
    "[&_.diff-split_.dl>:nth-child(3)]:border-l [&_.diff-split_.dl>:nth-child(3)]:border-line ",
    "[&_.diff-split_.sp-del]:bg-del-bg [&_.diff-split_.sp-del]:text-del-ink ",
    "[&_.diff-split_.sp-add]:bg-add-bg [&_.diff-split_.sp-add]:text-add-ink ",
    "[&_.diff-split_.sp-ctx]:text-ink-2 [&_.diff-split_.sp-pad]:bg-sunk ",
    "[&.commit-focus_.diff-split_.sp]:opacity-[.34] [&.commit-focus_.diff-split_.sp.owned]:opacity-100 [&.commit-focus_.diff-split_.sp.owned]:shadow-[inset_3px_0_0_var(--acc)] ",
    "[@media(max-width:1024px)]:[&_.diff-split_.dl]:grid-cols-[44px_minmax(0,1fr)] ",
    "[@media(max-width:1024px)]:[&_.diff-split_:is(.dl-meta,.dl-hunk)]:grid-cols-[minmax(0,1fr)] ",
    "[@media(max-width:1024px)]:[&_.diff-split_.dl>:is(:nth-child(3),:nth-child(4))]:border-t ",
    "[@media(max-width:1024px)]:[&_.diff-split_.dl>:is(:nth-child(3),:nth-child(4))]:border-line ",
    "[@media(max-width:1024px)]:[&_.diff-split_.dl>:nth-child(3)]:border-l-0",
);

/// One side of a paired side-by-side row.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SplitCell {
    pub(super) no: u32,
    pub(super) text: String,
    pub(super) owner: Option<String>,
    pub(super) spans: Vec<Span>,
}

/// One side-by-side row: full-width meta/hunk lines, mirrored context, or an
/// old/new pair where a missing side marks the "no corresponding line" gap.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SplitRow {
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
    },
    Pair {
        old: Option<SplitCell>,
        new: Option<SplitCell>,
    },
}

/// Pair structured rows into side-by-side rows.
pub(super) fn split_rows(rows: &[Row]) -> Vec<SplitRow> {
    let mut out = Vec::with_capacity(rows.len());
    let mut dels: Vec<&Row> = Vec::new();
    let mut adds: Vec<&Row> = Vec::new();

    for row in rows {
        match row.kind {
            RowKind::Del => dels.push(row),
            RowKind::Add => adds.push(row),
            RowKind::Meta => {
                flush_pairs(&mut out, &mut dels, &mut adds);
                out.push(SplitRow::Meta {
                    text: row.text.clone(),
                });
            }
            RowKind::Hunk => {
                flush_pairs(&mut out, &mut dels, &mut adds);
                out.push(SplitRow::Hunk {
                    text: row.text.clone(),
                });
            }
            RowKind::Context => {
                flush_pairs(&mut out, &mut dels, &mut adds);
                out.push(SplitRow::Context {
                    old_no: row.old_no.unwrap_or(0),
                    new_no: row.new_no.unwrap_or(0),
                    text: row.text.clone(),
                });
            }
        }
    }
    flush_pairs(&mut out, &mut dels, &mut adds);

    out
}

/// Emit the buffered deletion/addition runs as index-paired rows (shorter side
/// padded), computing intra-line spans only for short paired lines, then clear
/// both buffers.
fn flush_pairs(out: &mut Vec<SplitRow>, dels: &mut Vec<&Row>, adds: &mut Vec<&Row>) {
    for i in 0..dels.len().max(adds.len()) {
        let del = dels.get(i).copied();
        let add = adds.get(i).copied();

        let spans = match (del, add) {
            (Some(d), Some(a))
                if long_line_len(&d.text).is_none() && long_line_len(&a.text).is_none() =>
            {
                changed_spans(line_body(&d.text), line_body(&a.text))
            }
            _ => LineSpans::default(),
        };

        out.push(SplitRow::Pair {
            old: del.map(|d| SplitCell {
                no: d.old_no.unwrap_or(0),
                text: d.text.clone(),
                owner: d.owner.clone(),
                spans: spans.old.clone(),
            }),
            new: add.map(|a| SplitCell {
                no: a.new_no.unwrap_or(0),
                text: a.text.clone(),
                owner: a.owner.clone(),
                spans: spans.new.clone(),
            }),
        });
    }
    dels.clear();
    adds.clear();
}

#[cfg(test)]
mod tests {
    use application::diffs::LineOwners;

    use super::{
        super::model::{MAX_LINE_COLS, derive_rows},
        *,
    };

    fn split(raw: &[&str]) -> Vec<SplitRow> {
        let lines: Vec<String> = raw.iter().map(ToString::to_string).collect();
        split_rows(&derive_rows(&lines, &LineOwners::default()))
    }

    #[test]
    fn pairs_del_and_add_runs_indexwise_with_padding() {
        let rows = split(&["@@ -1,3 +1,2 @@", "-a", "-b", "+c"]);

        assert!(matches!(rows[0], SplitRow::Hunk { .. }));
        let SplitRow::Pair {
            old: Some(o),
            new: Some(n),
        } = &rows[1]
        else {
            panic!("first pair row must have both sides");
        };
        assert_eq!((o.no, o.text.as_str()), (1, "-a"));
        assert_eq!((n.no, n.text.as_str()), (1, "+c"));

        let SplitRow::Pair {
            old: Some(o2),
            new: None,
        } = &rows[2]
        else {
            panic!("second del must be unpaired (padded new side)");
        };
        assert_eq!((o2.no, o2.text.as_str()), (2, "-b"));
    }

    #[test]
    fn context_rows_mirror_on_both_panes_and_flush_runs() {
        let rows = split(&["@@ -1,3 +1,3 @@", "-a", " mid", "+b"]);

        assert!(
            matches!(
                rows[1],
                SplitRow::Pair {
                    old: Some(_),
                    new: None
                }
            ),
            "del before context must flush unpaired"
        );
        let SplitRow::Context {
            old_no,
            new_no,
            text,
        } = &rows[2]
        else {
            panic!("context row expected");
        };
        assert_eq!((*old_no, *new_no, text.as_str()), (2, 1, " mid"));
        assert!(matches!(
            rows[3],
            SplitRow::Pair {
                old: None,
                new: Some(_)
            }
        ));
    }

    #[test]
    fn paired_short_lines_get_intraline_spans_on_both_sides() {
        let rows = split(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]);

        let SplitRow::Pair {
            old: Some(o),
            new: Some(n),
        } = &rows[1]
        else {
            panic!("pair expected");
        };
        assert_eq!(o.spans, vec![Span { start: 8, end: 9 }]);
        assert_eq!(n.spans, vec![Span { start: 8, end: 9 }]);
    }

    #[test]
    fn long_lines_and_unpaired_lines_get_no_spans() {
        let long_del = format!("-{}x", "a".repeat(MAX_LINE_COLS + 5));
        let long_add = format!("+{}y", "a".repeat(MAX_LINE_COLS + 5));
        let rows = split(&["@@ -1 +1 @@", &long_del, &long_add]);
        let SplitRow::Pair {
            old: Some(o),
            new: Some(n),
        } = &rows[1]
        else {
            panic!("pair expected");
        };
        assert!(o.spans.is_empty() && n.spans.is_empty());

        let rows = split(&["@@ -1 +1 @@", "-solo"]);
        let SplitRow::Pair {
            old: Some(o),
            new: None,
        } = &rows[1]
        else {
            panic!("unpaired del expected");
        };
        assert!(o.spans.is_empty());
    }

    #[test]
    fn owners_ride_along_on_cells() {
        let mut owners = LineOwners::default();
        owners.deleted.insert(1, "abc123def".into());
        let lines: Vec<String> = ["@@ -1 +1 @@", "-a"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let rows = split_rows(&derive_rows(&lines, &owners));
        let SplitRow::Pair { old: Some(o), .. } = &rows[1] else {
            panic!("pair expected");
        };
        assert_eq!(o.owner.as_deref(), Some("abc123def"));
    }

    #[test]
    fn trailing_runs_flush_at_end_of_input() {
        let rows = split(&["@@ -1 +1 @@", "-a", "+b"]);
        assert_eq!(rows.len(), 2, "hunk + one pair");
    }
}
