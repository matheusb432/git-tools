//! Diff row rendering: raw unified-diff lines to HTML row strings, in unified
//! and side-by-side layouts. Row derivation and pairing live in the child
//! modules; this module owns the HTML assembly shared by both layouts. Rows
//! emit semantic classes (`.dl`, `.sp`, `.ln`, `.ciw`) styled once by the
//! layout root, so per-row markup weight stays constant.

mod intraline;
mod model;
mod split;

use application::diffs::LineOwners;

use self::{
    intraline::Span,
    model::{Row, RowKind, derive_rows, long_line_len},
    split::{SplitRow, split_rows},
};
use crate::text::{escape_html, html_or_nbsp, push_escaped};

pub(crate) const ROW_PRESENTATION_CLASSES: &str = concat!(
    "[&_.dl]:grid [&_.dl]:grid-cols-[44px_44px_minmax(0,1fr)] [&_.dl]:items-start [&_.dl]:whitespace-normal ",
    "[&_.dl_.ln]:select-none [&_.dl_.ln]:whitespace-nowrap [&_.dl_.ln]:px-2 [&_.dl_.ln]:text-right [&_.dl_.ln]:text-[12px] [&_.dl_.ln]:text-ink-3 [&_.dl_.ln]:[font-variant-numeric:tabular-nums] ",
    "[&_.dl_code]:min-w-0 [&_.dl_code]:border-0 [&_.dl_code]:bg-transparent [&_.dl_code]:px-3 [&_.dl_code]:text-[14px] [&_.dl_code]:text-code [&_.dl_code]:whitespace-pre-wrap [&_.dl_code]:[overflow-wrap:anywhere] ",
    "[&_:is(.dl-long_code.long,.diff-split_code.long)]:flex [&_:is(.dl-long_code.long,.diff-split_code.long)]:items-baseline [&_:is(.dl-long_code.long,.diff-split_code.long)]:gap-2 ",
    "[&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:min-w-0 [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:flex-1 [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:overflow-hidden [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:text-ellipsis [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:whitespace-pre ",
    "[&_:is(.dl-long.expanded_.code-text,.diff-split_code.long.expanded_.code-text)]:overflow-x-auto [&_:is(.dl-long.expanded_.code-text,.diff-split_code.long.expanded_.code-text)]:text-clip ",
    "[&_.ln-more]:flex-none [&_.ln-more]:cursor-pointer [&_.ln-more]:select-none [&_.ln-more]:rounded-sm [&_.ln-more]:border [&_.ln-more]:border-acc-line [&_.ln-more]:bg-acc-soft [&_.ln-more]:px-1.5 [&_.ln-more]:text-[11px] [&_.ln-more]:text-acc [&_.ln-more]:[font:inherit] ",
    "[&_.ln-more:hover]:bg-acc [&_.ln-more:hover]:text-bg ",
    "[&_.dl-add]:bg-add-bg [&_.dl-add_.ln]:bg-add-gut [&_.dl-add_.ln]:text-add [&_.dl-add_code]:text-add-ink ",
    "[&_.dl-del]:bg-del-bg [&_.dl-del_.ln]:bg-del-gut [&_.dl-del_.ln]:text-del [&_.dl-del_code]:text-del-ink ",
    "[&_.dl-ctx_code]:text-ink-2 [&_.dl-hunk]:bg-sunk [&_.dl-hunk_code]:font-semibold [&_.dl-hunk_code]:text-ink-3 ",
    "[&_.dl-meta]:opacity-60 [&_.dl-meta_code]:text-ink-3 ",
    "[&.commit-focus_.diff-unified_.dl]:opacity-[.34] [&.commit-focus_.diff-unified_.dl.owned]:opacity-100 ",
    "[&.commit-focus_.diff-unified_:is(.dl-add,.dl-del).owned]:shadow-[inset_3px_0_0_var(--acc)]",
);

pub(crate) const SPLIT_PRESENTATION_CLASSES: &str = split::PRESENTATION_CLASSES;
pub(crate) const INTRALINE_PRESENTATION_CLASSES: &str = intraline::PRESENTATION_CLASSES;

pub(crate) fn render_diff_lines(lines: &[String], owners: &LineOwners) -> String {
    render_unified_rows(&derive_rows(lines, owners))
}

fn render_unified_rows(parsed: &[Row]) -> String {
    use std::fmt::Write;

    let mut rows =
        String::with_capacity(parsed.iter().map(|row| row.text.len()).sum::<usize>() * 2);

    for row in parsed {
        match row.kind {
            RowKind::Meta => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                    html_or_nbsp(&row.text)
                );
            }
            RowKind::Hunk => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                    escape_html(&row.text)
                );
            }
            RowKind::Add => {
                let long = long_line_len(&row.text);
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-add{}"{}><span class="ln"></span><span class="ln">{}</span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    commit_attr(row.owner.as_ref()),
                    row.new_no.unwrap_or(0),
                    code_cell(&row.text, long),
                );
            }
            RowKind::Del => {
                let long = long_line_len(&row.text);
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-del{}"{}><span class="ln">{}</span><span class="ln"></span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    commit_attr(row.owner.as_ref()),
                    row.old_no.unwrap_or(0),
                    code_cell(&row.text, long),
                );
            }
            RowKind::Context => {
                let long = long_line_len(&row.text);
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-ctx{}"><span class="ln">{}</span><span class="ln">{}</span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    row.old_no.unwrap_or(0),
                    row.new_no.unwrap_or(0),
                    code_cell(&row.text, long),
                );
            }
        }
    }

    rows
}

// Side-by-side (VS Code-style) counterpart of `render_diff_lines`: the same parsed diff laid
// out as old | new panes. Within a hunk, a run of deletions is paired index-wise with the
// following run of additions (the shorter side padded), context lines mirror on both panes,
// and meta/hunk headers span the full width. Same gutter-number tracking and owner tagging.
pub(crate) fn render_diff_split(lines: &[String], owners: &LineOwners) -> String {
    render_split_rows(&split_rows(&derive_rows(lines, owners)))
}

fn render_split_rows(parsed: &[SplitRow]) -> String {
    use std::fmt::Write;

    let mut rows = String::with_capacity(parsed.len() * 128);

    for row in parsed {
        match row {
            SplitRow::Meta { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-meta"><code>{}</code></div>"#,
                    html_or_nbsp(text)
                );
            }
            SplitRow::Hunk { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-hunk"><code>{}</code></div>"#,
                    escape_html(text)
                );
            }
            SplitRow::Context {
                old_no,
                new_no,
                text,
            } => {
                let long = long_line_len(text);
                let _ = write!(rows, r#"<div class="dl"><span class="ln">{old_no}</span>"#);
                push_split_code(&mut rows, text, long, "sp-ctx", None, &[]);
                let _ = write!(rows, r#"<span class="ln">{new_no}</span>"#);
                push_split_code(&mut rows, text, long, "sp-ctx", None, &[]);
                rows.push_str("</div>");
            }
            SplitRow::Pair { old, new } => {
                rows.push_str(r#"<div class="dl">"#);
                match &old {
                    Some(cell) => {
                        let _ = write!(rows, r#"<span class="ln">{}</span>"#, cell.no);
                        push_split_code(
                            &mut rows,
                            &cell.text,
                            long_line_len(&cell.text),
                            "sp-del",
                            cell.owner.as_ref(),
                            &cell.spans,
                        );
                    }
                    None => rows.push_str(SPLIT_PAD),
                }
                match &new {
                    Some(cell) => {
                        let _ = write!(rows, r#"<span class="ln">{}</span>"#, cell.no);
                        push_split_code(
                            &mut rows,
                            &cell.text,
                            long_line_len(&cell.text),
                            "sp-add",
                            cell.owner.as_ref(),
                            &cell.spans,
                        );
                    }
                    None => rows.push_str(SPLIT_PAD),
                }
                rows.push_str("</div>");
            }
        }
    }

    rows
}

// ! Short shas are [0-9a-f]{9} (no HTML metacharacters), so no escaping is needed.
fn commit_attr(sha: Option<&String>) -> String {
    sha.map(|sha| format!(r#" data-commit="{sha}""#))
        .unwrap_or_default()
}

// ! Inner content of a `<code>` cell: the bare body, or for a tamed long line, the copy-safe
// ! `.code-text` span plus the expander button. Shared by `code_cell` and `push_split_code`.
fn code_inner(raw: &str, long: Option<usize>) -> String {
    let body = html_or_nbsp(raw);
    match long {
        None => body,
        Some(len) => format!(
            r#"<span class="code-text">{body}</span><button class="ln-more" type="button" aria-expanded="false">⋯ {len} chars</button>"#
        ),
    }
}

// ! Long lines (e.g. base64 data URIs) would force char-by-char wrap layout and freeze the
// ! page. Tame them: full text stays in `.code-text` (copy-safe) but renders clipped/no-wrap,
// ! with an expander that reveals horizontal scroll. `long` is the precomputed Some(len).
fn code_cell(raw: &str, long: Option<usize>) -> String {
    let inner = code_inner(raw, long);
    if long.is_some() {
        format!(r#"<code class="long">{inner}</code>"#)
    } else {
        format!("<code>{inner}</code>")
    }
}

// ! Append one side of a side-by-side row directly to the shared row buffer.
fn push_split_code(
    out: &mut String,
    raw: &str,
    long: Option<usize>,
    side: &str,
    commit: Option<&String>,
    spans: &[Span],
) {
    out.push_str(r#"<code class="sp "#);
    out.push_str(side);
    if long.is_some() {
        out.push_str(" long");
    }
    out.push('"');
    if let Some(commit) = commit {
        out.push_str(" data-commit=\"");
        out.push_str(commit);
        out.push('"');
    }
    out.push('>');

    if long.is_some() {
        out.push_str(&code_inner(raw, long));
    } else if spans.is_empty() {
        if raw.is_empty() {
            out.push_str("&nbsp;");
        } else {
            for ch in raw.chars() {
                push_escaped(out, ch);
            }
        }
    } else {
        out.push_str(&mark_spans(raw, spans));
    }
    out.push_str("</code>");
}

// ! Escape a changed line body char by char, wrapping the `spans` (char-index ranges into the
// ! body, i.e. after the leading +/- marker) in `.ciw` so CSS can paint the VS Code-style
// ! intra-line highlight. The marker char is ASCII +/- (never escaped) and is never wrapped.
fn mark_spans(raw: &str, spans: &[Span]) -> String {
    let mut out = String::with_capacity(raw.len() + spans.len() * 26);
    let mut chars = raw.chars();
    if let Some(marker) = chars.next() {
        out.push(marker);
    }

    let mut open = false;
    for (i, ch) in chars.enumerate() {
        let inside = spans.iter().any(|span| i >= span.start && i < span.end);
        if inside && !open {
            out.push_str(r#"<span class="ciw">"#);
            open = true;
        } else if !inside && open {
            out.push_str("</span>");
            open = false;
        }
        push_escaped(&mut out, ch);
    }
    if open {
        out.push_str("</span>");
    }
    out
}

// ! An empty side of a side-by-side row — no line on this pane, so a blank gutter and a
// ! filler cell that CSS shades to mark the gap (VS Code's "no corresponding line").
const SPLIT_PAD: &str = r#"<span class="ln"></span><code class="sp sp-pad"></code>"#;

#[cfg(test)]
mod tests {
    use super::{
        model::MAX_LINE_COLS,
        split::{SplitCell, SplitRow},
        *,
    };

    #[test]
    fn render_unified_rows_maps_each_row_kind_and_gutter() {
        let html = render_unified_rows(&[
            Row {
                kind: RowKind::Meta,
                old_no: None,
                new_no: None,
                text: "index 111..222 100644".into(),
                owner: None,
            },
            Row {
                kind: RowKind::Hunk,
                old_no: None,
                new_no: None,
                text: "@@ -3,2 +7,2 @@".into(),
                owner: None,
            },
            Row {
                kind: RowKind::Context,
                old_no: Some(3),
                new_no: Some(7),
                text: " keep".into(),
                owner: None,
            },
            Row {
                kind: RowKind::Del,
                old_no: Some(4),
                new_no: None,
                text: "-old".into(),
                owner: None,
            },
            Row {
                kind: RowKind::Add,
                old_no: None,
                new_no: Some(8),
                text: "+new".into(),
                owner: None,
            },
        ]);

        assert!(html.contains(r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>index 111..222 100644</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>@@ -3,2 +7,2 @@</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">3</span><span class="ln">7</span><code> keep</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-del"><span class="ln">4</span><span class="ln"></span><code>-old</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-add"><span class="ln"></span><span class="ln">8</span><code>+new</code></div>"#));
    }

    #[test]
    fn render_unified_rows_maps_commit_owners_to_attributes() {
        let html = render_unified_rows(&[
            Row {
                kind: RowKind::Context,
                old_no: Some(3),
                new_no: Some(7),
                text: " keep".into(),
                owner: None,
            },
            Row {
                kind: RowKind::Del,
                old_no: Some(4),
                new_no: None,
                text: "-old".into(),
                owner: Some("fff000aaa".into()),
            },
            Row {
                kind: RowKind::Add,
                old_no: None,
                new_no: Some(8),
                text: "+new".into(),
                owner: Some("abc123def".into()),
            },
        ]);
        assert!(html.contains(r#"<div class="dl dl-del" data-commit="fff000aaa">"#));
        assert!(html.contains(r#"<div class="dl dl-add" data-commit="abc123def">"#));
        // context rows carry no owner attribute
        assert!(html.contains(r#"<div class="dl dl-ctx"><span"#));
    }

    #[test]
    fn render_unified_rows_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_unified_rows(&[Row {
            kind: RowKind::Add,
            old_no: None,
            new_no: Some(1),
            text: long,
            owner: None,
        }]);
        assert!(html.contains(r#"class="dl dl-add dl-long""#));
        assert!(html.contains(r#"<span class="code-text">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            MAX_LINE_COLS + 5
        )));
    }

    #[test]
    fn render_unified_rows_leaves_normal_lines_untamed() {
        let html = render_unified_rows(&[Row {
            kind: RowKind::Add,
            old_no: None,
            new_no: Some(1),
            text: "+short".into(),
            owner: None,
        }]);
        assert!(!html.contains("dl-long"));
        assert!(!html.contains("code-text"));
    }

    #[test]
    fn rendered_rows_keep_enhancer_and_presentation_hooks() {
        let unified = render_unified_rows(&[
            Row {
                kind: RowKind::Add,
                old_no: None,
                new_no: Some(1),
                text: format!("+{}", "a".repeat(MAX_LINE_COLS + 5)),
                owner: Some("abc123def".into()),
            },
            Row {
                kind: RowKind::Del,
                old_no: Some(1),
                new_no: None,
                text: "-old".into(),
                owner: Some("abc123def".into()),
            },
        ]);
        let split = render_split_rows(&[SplitRow::Pair {
            old: Some(SplitCell {
                no: 1,
                text: "-old".into(),
                owner: Some("abc123def".into()),
                spans: vec![Span { start: 0, end: 1 }],
            }),
            new: Some(SplitCell {
                no: 1,
                text: "+new".into(),
                owner: Some("abc123def".into()),
                spans: vec![Span { start: 0, end: 1 }],
            }),
        }]);

        for hook in ["dl", "dl-add", "dl-del", "dl-long", "ln", "code-text"] {
            assert!(unified.contains(hook), "unified rows lost {hook}");
        }
        for hook in ["dl", "ln", "sp", "sp-add", "sp-del", "ciw"] {
            assert!(split.contains(hook), "split rows lost {hook}");
        }
        assert!(unified.contains(r#"data-commit="abc123def""#));
        assert!(split.contains(r#"data-commit="abc123def""#));
    }

    #[test]
    fn render_split_rows_maps_meta_hunk_context_and_pairs() {
        let html = render_split_rows(&[
            SplitRow::Meta {
                text: "index 111..222 100644".into(),
            },
            SplitRow::Hunk {
                text: "@@ -3,2 +7,2 @@".into(),
            },
            SplitRow::Context {
                old_no: 3,
                new_no: 7,
                text: " keep".into(),
            },
            SplitRow::Pair {
                old: Some(SplitCell {
                    no: 4,
                    text: "-old".into(),
                    owner: None,
                    spans: vec![],
                }),
                new: Some(SplitCell {
                    no: 8,
                    text: "+new".into(),
                    owner: None,
                    spans: vec![],
                }),
            },
        ]);

        // meta + hunk headers span the full width (single cell, no gutters)
        assert!(
            html.contains(r#"<div class="dl dl-meta"><code>index 111..222 100644</code></div>"#)
        );
        assert!(html.contains(r#"<div class="dl dl-hunk"><code>@@ -3,2 +7,2 @@</code></div>"#));
        // a context line mirrors onto both panes with each pane's gutter number
        assert!(html.contains(r#"<div class="dl"><span class="ln">3</span><code class="sp sp-ctx"> keep</code><span class="ln">7</span><code class="sp sp-ctx"> keep</code></div>"#));
        // a deletion pairs with the following addition on one row: old pane | new pane
        assert!(html.contains(r#"<div class="dl"><span class="ln">4</span><code class="sp sp-del">-old</code><span class="ln">8</span><code class="sp sp-add">+new</code></div>"#));
    }

    #[test]
    fn render_split_rows_maps_a_missing_side_to_padding() {
        let html = render_split_rows(&[SplitRow::Pair {
            old: Some(SplitCell {
                no: 2,
                text: "-b".into(),
                owner: None,
                spans: vec![],
            }),
            new: None,
        }]);

        assert!(html.contains(r#"<div class="dl"><span class="ln">2</span><code class="sp sp-del">-b</code><span class="ln"></span><code class="sp sp-pad"></code></div>"#));
    }

    #[test]
    fn render_split_rows_maps_commit_owners_to_each_side() {
        let html = render_split_rows(&[SplitRow::Pair {
            old: Some(SplitCell {
                no: 4,
                text: "-old".into(),
                owner: Some("fff000aaa".into()),
                spans: vec![],
            }),
            new: Some(SplitCell {
                no: 8,
                text: "+new".into(),
                owner: Some("abc123def".into()),
                spans: vec![],
            }),
        }]);

        assert!(html.contains(r#"<code class="sp sp-del" data-commit="fff000aaa">-old</code>"#));
        assert!(html.contains(r#"<code class="sp sp-add" data-commit="abc123def">+new</code>"#));
    }

    #[test]
    fn split_code_appends_exact_markup_to_the_row_buffer() {
        let mut html = String::from("prefix");

        push_split_code(
            &mut html,
            "+new<&",
            None,
            "sp-add",
            Some(&"abc123def".to_string()),
            &[],
        );

        assert_eq!(
            html,
            r#"prefix<code class="sp sp-add" data-commit="abc123def">+new&lt;&amp;</code>"#
        );
    }

    #[test]
    fn render_split_rows_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_split_rows(&[SplitRow::Pair {
            old: None,
            new: Some(SplitCell {
                no: 1,
                text: long,
                owner: None,
                spans: vec![],
            }),
        }]);
        assert!(html.contains(r#"<code class="sp sp-add long">"#));
        assert!(html.contains(r#"<span class="code-text">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            MAX_LINE_COLS + 5
        )));
    }

    #[test]
    fn render_split_rows_maps_intraline_spans_to_markup() {
        let html = render_split_rows(&[SplitRow::Pair {
            old: Some(SplitCell {
                no: 1,
                text: "-let x = 1;".into(),
                owner: None,
                spans: vec![Span { start: 8, end: 9 }],
            }),
            new: Some(SplitCell {
                no: 1,
                text: "+let x = 2;".into(),
                owner: None,
                spans: vec![Span { start: 8, end: 9 }],
            }),
        }]);

        // only the differing char is wrapped; the shared prefix/suffix stay bare
        assert!(
            html.contains(r#"<code class="sp sp-del">-let x = <span class="ciw">1</span>;</code>"#)
        );
        assert!(
            html.contains(r#"<code class="sp sp-add">+let x = <span class="ciw">2</span>;</code>"#)
        );
    }

    #[test]
    fn render_split_rows_escapes_marked_user_controlled_chars() {
        let html = render_split_rows(&[SplitRow::Pair {
            old: Some(SplitCell {
                no: 1,
                text: "-a<b>&1".into(),
                owner: None,
                spans: vec![Span { start: 5, end: 6 }],
            }),
            new: None,
        }]);

        // the marked char stays escaped inside the span; no raw `<b>` leaks
        assert!(html.contains(r#"a&lt;b&gt;&amp;<span class="ciw">1</span>"#));
        assert!(!html.contains("<b>"));
    }
}
