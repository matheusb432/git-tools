//! Diff row rendering: raw unified-diff lines to HTML row strings, in unified
//! and side-by-side layouts. `gtl-parser` owns semantic row derivation and
//! pairing; this module owns the HTML assembly shared by both layouts. Rows
//! emit semantic classes (`.dl`, `.sp`, `.ln`, `.ciw`) styled once by the
//! layout root, so per-row markup weight stays constant.

use gtl_parser::{
    CharacterSpan, DiffParser, DiffRow, DiffRowKind, SplitDiffRow, SyntaxDefinition, SyntaxToken,
    diff_line_body,
};

use crate::{
    syntax::token_css_class,
    text::{escape_html, html_or_nbsp, push_escaped},
};

pub(crate) const ROW_PRESENTATION_CLASSES: &str = concat!(
    "[&_.dl]:grid [&_.dl]:grid-cols-[44px_44px_minmax(0,1fr)] [&_.dl]:items-start [&_.dl]:whitespace-normal ",
    "[&_.dl_.ln]:select-none [&_.dl_.ln]:whitespace-nowrap [&_.dl_.ln]:px-2 [&_.dl_.ln]:text-right [&_.dl_.ln]:text-[12px] [&_.dl_.ln]:text-ink-3 [&_.dl_.ln]:[font-variant-numeric:tabular-nums] ",
    "[&_.dl_code]:min-w-0 [&_.dl_code]:border-0 [&_.dl_code]:bg-transparent [&_.dl_code]:px-3 [&_.dl_code]:text-[14px] [&_.dl_code]:text-code [&_.dl_code]:whitespace-pre-wrap [&_.dl_code]:[overflow-wrap:anywhere] ",
    "[&_:is(.dl-long_code.long,.diff-split_code.long)]:flex [&_:is(.dl-long_code.long,.diff-split_code.long)]:items-baseline [&_:is(.dl-long_code.long,.diff-split_code.long)]:gap-2 ",
    "[&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:min-w-0 [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:flex-1 [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:overflow-hidden [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:text-ellipsis [&_:is(.dl-long_.code-text,.diff-split_code.long_.code-text)]:whitespace-pre ",
    "[&_:is(.dl-long.expanded_.code-text,.diff-split_code.long.expanded_.code-text)]:overflow-x-auto [&_:is(.dl-long.expanded_.code-text,.diff-split_code.long.expanded_.code-text)]:text-clip ",
    "[&_.ln-more]:flex-none [&_.ln-more]:cursor-pointer [&_.ln-more]:select-none [&_.ln-more]:rounded-sm [&_.ln-more]:border [&_.ln-more]:border-acc-line [&_.ln-more]:bg-acc-soft [&_.ln-more]:px-1.5 [&_.ln-more]:text-[11px] [&_.ln-more]:text-acc [&_.ln-more]:[font:inherit] ",
    "[&_.ln-more:hover]:bg-acc [&_.ln-more:hover]:text-bg ",
    "[&_.dl-add]:bg-add-bg [&_.dl-add_.ln]:bg-add-gut [&_.dl-add_.ln]:text-add ",
    "[&_.dl-del]:bg-del-bg [&_.dl-del_.ln]:bg-del-gut [&_.dl-del_.ln]:text-del ",
    "[&_.dl-hunk]:bg-sunk [&_.dl-hunk_code]:font-semibold [&_.dl-hunk_code]:text-ink-3 ",
    "[&_.dl-meta]:opacity-60 [&_.dl-meta_code]:text-ink-3 ",
);

pub(crate) const SPLIT_PRESENTATION_CLASSES: &str = concat!(
    "[&_.diff-split_.dl]:grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] [&_.diff-split_.dl]:items-stretch ",
    "[&_.diff-split_:is(.dl-meta,.dl-hunk)]:grid-cols-[minmax(0,1fr)] ",
    "[&_.diff-split_.dl>:nth-child(3)]:border-l [&_.diff-split_.dl>:nth-child(3)]:border-line ",
    "[&_.diff-split_.sp-del]:bg-del-bg ",
    "[&_.diff-split_.sp-add]:bg-add-bg ",
    "[&_.diff-split_.sp-pad]:bg-sunk ",
    "tablet:[&_.diff-split_.dl]:grid-cols-[44px_minmax(0,1fr)] ",
    "tablet:[&_.diff-split_:is(.dl-meta,.dl-hunk)]:grid-cols-[minmax(0,1fr)] ",
    "tablet:[&_.diff-split_.dl>:is(:nth-child(3),:nth-child(4))]:border-t ",
    "tablet:[&_.diff-split_.dl>:is(:nth-child(3),:nth-child(4))]:border-line ",
    "tablet:[&_.diff-split_.dl>:nth-child(3)]:border-l-0",
);
pub(crate) const INTRALINE_PRESENTATION_CLASSES: &str = concat!(
    "[&_.diff-split_:is(.sp-del,.sp-add)_.ciw]:rounded-[2px] ",
    "[&_.diff-split_.sp-del_.ciw]:bg-[color-mix(in_srgb,var(--del)_34%,transparent)] ",
    "[&_.diff-split_.sp-add_.ciw]:bg-[color-mix(in_srgb,var(--add)_34%,transparent)]",
);

pub(crate) fn unified_line_number_digits(lines: &[String]) -> u32 {
    DiffParser::new().parse(lines).line_number_digits()
}

pub(crate) fn render_diff_lines(lines: &[String], syntax: Option<SyntaxDefinition>) -> String {
    let parsed = DiffParser::new().with_syntax(syntax).parse(lines);
    render_unified_rows(parsed.rows())
}

fn render_unified_rows(parsed: &[DiffRow]) -> String {
    use std::fmt::Write;

    let mut rows =
        String::with_capacity(parsed.iter().map(|row| row.text().len()).sum::<usize>() * 2);

    for row in parsed {
        match row.kind() {
            DiffRowKind::Meta => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                    html_or_nbsp(row.text())
                );
            }
            DiffRowKind::Hunk => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                    escape_html(row.text())
                );
            }
            DiffRowKind::Added => {
                let long = row.long_line_character_count();
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-add{}"><span class="ln"></span><span class="ln">{}</span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    row.new_line_number().unwrap_or(0),
                    code_cell(row.text(), long, row.syntax_tokens()),
                );
            }
            DiffRowKind::Removed => {
                let long = row.long_line_character_count();
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-del{}"><span class="ln">{}</span><span class="ln"></span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    row.old_line_number().unwrap_or(0),
                    code_cell(row.text(), long, row.syntax_tokens()),
                );
            }
            DiffRowKind::Context => {
                let long = row.long_line_character_count();
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-ctx{}"><span class="ln">{}</span><span class="ln">{}</span>{}</div>"#,
                    if long.is_some() { " dl-long" } else { "" },
                    row.old_line_number().unwrap_or(0),
                    row.new_line_number().unwrap_or(0),
                    code_cell(row.text(), long, row.syntax_tokens()),
                );
            }
        }
    }

    rows
}

// Side-by-side (VS Code-style) counterpart of `render_diff_lines`: the same parsed diff laid
// out as old | new panes. Within a hunk, a run of deletions is paired index-wise with the
// following run of additions (the shorter side padded), context lines mirror on both panes,
// and meta/hunk headers span the full width.
pub(crate) fn render_diff_split(lines: &[String], syntax: Option<SyntaxDefinition>) -> String {
    let parsed = DiffParser::new().with_syntax(syntax).parse(lines);
    render_split_rows(&parsed.split_rows())
}

fn render_split_rows(parsed: &[SplitDiffRow]) -> String {
    use std::fmt::Write;

    let mut rows = String::with_capacity(parsed.len() * 128);

    for row in parsed {
        match row {
            SplitDiffRow::Meta { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-meta"><code>{}</code></div>"#,
                    html_or_nbsp(text)
                );
            }
            SplitDiffRow::Hunk { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-hunk"><code>{}</code></div>"#,
                    escape_html(text)
                );
            }
            SplitDiffRow::Context {
                old_line_number,
                new_line_number,
                text,
                syntax_tokens,
                long_line_character_count,
                ..
            } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl"><span class="ln">{old_line_number}</span>"#
                );
                push_split_code(
                    &mut rows,
                    text,
                    *long_line_character_count,
                    "sp-ctx",
                    syntax_tokens,
                    &[],
                );
                let _ = write!(rows, r#"<span class="ln">{new_line_number}</span>"#);
                push_split_code(
                    &mut rows,
                    text,
                    *long_line_character_count,
                    "sp-ctx",
                    syntax_tokens,
                    &[],
                );
                rows.push_str("</div>");
            }
            SplitDiffRow::Pair { old, new } => {
                rows.push_str(r#"<div class="dl">"#);
                match &old {
                    Some(cell) => {
                        let _ = write!(rows, r#"<span class="ln">{}</span>"#, cell.line_number());
                        push_split_code(
                            &mut rows,
                            cell.text(),
                            cell.long_line_character_count(),
                            "sp-del",
                            cell.syntax_tokens(),
                            cell.intraline_spans(),
                        );
                    }
                    None => rows.push_str(SPLIT_PAD),
                }
                match &new {
                    Some(cell) => {
                        let _ = write!(rows, r#"<span class="ln">{}</span>"#, cell.line_number());
                        push_split_code(
                            &mut rows,
                            cell.text(),
                            cell.long_line_character_count(),
                            "sp-add",
                            cell.syntax_tokens(),
                            cell.intraline_spans(),
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

// ! Inner content of a `<code>` cell: the bare body, or for a tamed long line, the copy-safe
// ! `.code-text` span plus the expander button. Shared by `code_cell` and `push_split_code`.
fn code_inner(raw: &str, long: Option<usize>, tokens: &[SyntaxToken]) -> String {
    match long {
        None if tokens.is_empty() => html_or_nbsp(raw),
        None => {
            let mut body = String::with_capacity(raw.len() * 2);
            push_body(&mut body, raw, tokens, &[]);
            body
        }
        Some(len) => format!(
            r#"<span class="code-text gtl-scroll-rail">{}</span><button class="ln-more" type="button" aria-expanded="false">⋯ {len} chars</button>"#,
            html_or_nbsp(raw)
        ),
    }
}

// ! Long lines (e.g. base64 data URIs) would force char-by-char wrap layout and freeze the
// ! page. Tame them: full text stays in `.code-text` (copy-safe) but renders clipped/no-wrap,
// ! with an expander that reveals horizontal scroll. `long` is the precomputed Some(len).
fn code_cell(raw: &str, long: Option<usize>, tokens: &[SyntaxToken]) -> String {
    let inner = code_inner(diff_line_body(raw), long, tokens);
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
    tokens: &[SyntaxToken],
    spans: &[CharacterSpan],
) {
    out.push_str(r#"<code class="sp "#);
    out.push_str(side);
    if long.is_some() {
        out.push_str(" long");
    }
    out.push('"');
    out.push('>');

    if long.is_some() {
        out.push_str(&code_inner(raw, long, &[]));
    } else if tokens.is_empty() && spans.is_empty() {
        if raw.is_empty() {
            out.push_str("&nbsp;");
        } else {
            for ch in raw.chars() {
                push_escaped(out, ch);
            }
        }
    } else {
        if let Some(marker) = raw.chars().next() {
            push_escaped(out, marker);
        }
        push_body(out, diff_line_body(raw), tokens, spans);
    }
    out.push_str("</code>");
}

// ! Escape a marker-free line body char by char, wrapping syntax token runs and intra-line
// ! changed runs in flat spans. A span closes and reopens wherever either range set changes,
// ! so tags never nest or overlap.
fn push_body(out: &mut String, body: &str, tokens: &[SyntaxToken], spans: &[CharacterSpan]) {
    let mut open = false;
    let mut current: (Option<&'static str>, bool) = (None, false);
    for (i, ch) in body.chars().enumerate() {
        let class = tokens
            .iter()
            .find(|token| i >= token.start() && i < token.end())
            .map(|token| token_css_class(token.class()));
        let ciw = spans.iter().any(|span| i >= span.start() && i < span.end());
        if (class, ciw) != current {
            if open {
                out.push_str("</span>");
                open = false;
            }
            current = (class, ciw);
            match (class, ciw) {
                (None, false) => {}
                (Some(class), false) => {
                    out.push_str(r#"<span class=""#);
                    out.push_str(class);
                    out.push_str(r#"">"#);
                    open = true;
                }
                (Some(class), true) => {
                    out.push_str(r#"<span class=""#);
                    out.push_str(class);
                    out.push_str(r#" ciw">"#);
                    open = true;
                }
                (None, true) => {
                    out.push_str(r#"<span class="ciw">"#);
                    open = true;
                }
            }
        }
        push_escaped(out, ch);
    }
    if open {
        out.push_str("</span>");
    }
}

// ! An empty side of a side-by-side row — no line on this pane, so a blank gutter and a
// ! filler cell that CSS shades to mark the gap (VS Code's "no corresponding line").
const SPLIT_PAD: &str = r#"<span class="ln"></span><code class="sp sp-pad"></code>"#;

#[cfg(test)]
mod tests {
    use gtl_parser::{DEFAULT_MAX_LINE_CHARACTERS, SyntaxTokenClass};

    use super::*;
    use crate::test_render::syntax_for_path;

    #[test]
    fn render_unified_rows_maps_each_row_kind_and_gutter() {
        let html = render_diff_lines(
            &diff_lines(&[
                "index 111..222 100644",
                "@@ -3,2 +7,2 @@",
                " keep",
                "-old",
                "+new",
            ]),
            None,
        );

        assert!(html.contains(r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>index 111..222 100644</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>@@ -3,2 +7,2 @@</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">3</span><span class="ln">7</span><code>keep</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-del"><span class="ln">4</span><span class="ln"></span><code>old</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-add"><span class="ln"></span><span class="ln">8</span><code>new</code></div>"#));
    }

    #[test]
    fn render_unified_rows_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(DEFAULT_MAX_LINE_CHARACTERS + 5));
        let html = render_diff_lines(&diff_lines(&["@@ -1 +1 @@", &long]), None);
        assert!(html.contains(r#"class="dl dl-add dl-long""#));
        assert!(html.contains(r#"<span class="code-text gtl-scroll-rail">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            DEFAULT_MAX_LINE_CHARACTERS + 5
        )));
    }

    #[test]
    fn render_unified_rows_leaves_normal_lines_untamed() {
        let html = render_diff_lines(&diff_lines(&["@@ -1 +1 @@", "+short"]), None);
        assert!(!html.contains("dl-long"));
        assert!(!html.contains("code-text"));
    }

    #[test]
    fn rendered_rows_keep_enhancer_and_presentation_hooks() {
        let long = format!("+{}", "a".repeat(DEFAULT_MAX_LINE_CHARACTERS + 5));
        let unified = render_diff_lines(&diff_lines(&["@@ -1 +1 @@", "-old", &long]), None);
        let split = render_diff_split(&diff_lines(&["@@ -1 +1 @@", "-same 1", "+same 2"]), None);

        for hook in ["dl", "dl-add", "dl-del", "dl-long", "ln", "code-text"] {
            assert!(unified.contains(hook), "unified rows lost {hook}");
        }
        for hook in ["dl", "ln", "sp", "sp-add", "sp-del", "ciw"] {
            assert!(split.contains(hook), "split rows lost {hook}");
        }
    }

    #[test]
    fn render_split_rows_maps_meta_hunk_context_and_pairs() {
        let html = render_diff_split(
            &diff_lines(&[
                "index 111..222 100644",
                "@@ -3,2 +7,2 @@",
                " keep",
                "-old",
                "+new",
            ]),
            None,
        );

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
        let html = render_diff_split(&diff_lines(&["@@ -1,2 +1 @@", "-a", "-b", "+c"]), None);

        assert!(html.contains(r#"<div class="dl"><span class="ln">2</span><code class="sp sp-del">-b</code><span class="ln"></span><code class="sp sp-pad"></code></div>"#));
    }

    #[test]
    fn split_code_appends_exact_markup_to_the_row_buffer() {
        let mut html = String::from("prefix");

        push_split_code(&mut html, "+new<&", None, "sp-add", &[], &[]);

        assert_eq!(
            html,
            r#"prefix<code class="sp sp-add">+new&lt;&amp;</code>"#
        );
    }

    #[test]
    fn render_split_rows_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(DEFAULT_MAX_LINE_CHARACTERS + 5));
        let html = render_diff_split(&diff_lines(&["@@ -0,0 +1 @@", &long]), None);
        assert!(html.contains(r#"<code class="sp sp-add long">"#));
        assert!(html.contains(r#"<span class="code-text gtl-scroll-rail">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            DEFAULT_MAX_LINE_CHARACTERS + 5
        )));
    }

    #[test]
    fn render_split_rows_maps_intraline_spans_to_markup() {
        let html = render_diff_split(
            &diff_lines(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]),
            None,
        );

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
        let html = render_diff_split(&diff_lines(&["@@ -1 +1 @@", "-a<b>&1", "+a<b>&2"]), None);

        // the marked char stays escaped inside the span; no raw `<b>` leaks
        assert!(html.contains(r#"a&lt;b&gt;&amp;<span class="ciw">1</span>"#));
        assert!(!html.contains("<b>"));
    }

    fn diff_lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    /// Parse a rendered rows fragment, failing on any HTML parse error, and
    /// return the entity-decoded text of every element matching `selector`.
    fn select_texts(html: &str, selector: &str) -> Vec<String> {
        let fragment = scraper::Html::parse_fragment(html);
        assert!(
            fragment.errors.is_empty(),
            "invalid html: {:?}\n{html}",
            fragment.errors
        );
        let selector = scraper::Selector::parse(selector).unwrap();
        fragment
            .select(&selector)
            .map(|element| element.text().collect::<String>())
            .collect()
    }

    fn token_texts(html: &str, class: SyntaxTokenClass) -> Vec<String> {
        select_texts(html, &format!("span.{}", token_css_class(class)))
    }

    #[test]
    fn unified_rows_carry_syntax_token_spans() {
        let html = render_diff_lines(
            &diff_lines(&["@@ -1 +1 @@", "+let x = 1; // note"]),
            syntax_for_path("a.rs"),
        );
        assert!(
            token_texts(&html, SyntaxTokenClass::Number).contains(&"1".to_string()),
            "{html}"
        );
        assert!(
            token_texts(&html, SyntaxTokenClass::Comment).contains(&"// note".to_string()),
            "{html}"
        );
    }

    #[test]
    fn unified_rows_omit_diff_marker_without_dropping_source_punctuation() {
        let html = render_diff_lines(
            &diff_lines(&["@@ -1 +1 @@", "+#[cfg(test)]"]),
            syntax_for_path("a.rs"),
        );

        assert_eq!(
            select_texts(&html, ".dl-add code"),
            ["#[cfg(test)]".to_string()]
        );
    }

    #[test]
    fn token_spans_escape_their_content() {
        let html = render_diff_lines(
            &diff_lines(&["@@ -1 +1 @@", r#"+let s = "<&>";"#]),
            syntax_for_path("a.rs"),
        );
        // element text is entity-decoded: recovering the raw metacharacters
        // proves they were escaped in the markup, and a leaked `<` fails the
        // helper's parse-error assertion.
        assert!(
            token_texts(&html, SyntaxTokenClass::String).contains(&r#""<&>""#.to_string()),
            "{html}"
        );
    }

    #[test]
    fn split_rows_combine_token_and_intraline_classes_on_one_flat_span() {
        let html = render_diff_split(
            &diff_lines(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]),
            syntax_for_path("a.rs"),
        );
        let combined = select_texts(
            &html,
            &format!("span.{}.ciw", token_css_class(SyntaxTokenClass::Number)),
        );
        assert!(combined.contains(&"1".to_string()), "{html}");
        assert!(combined.contains(&"2".to_string()), "{html}");
    }

    #[test]
    fn unknown_syntax_renders_exactly_plain_rows() {
        let html = render_diff_lines(&diff_lines(&["@@ -1 +1 @@", "+let x = 1;"]), None);
        assert!(
            select_texts(&html, r#"span[class*="sy-"]"#).is_empty(),
            "{html}"
        );
        assert!(html.contains(r"<code>let x = 1;</code>"), "{html}");
    }
}
