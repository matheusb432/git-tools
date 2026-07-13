use domain::{
    diffs::{FileDiff, LineOwners, RowKind, Span, SplitRow, View, long_line_len},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};

const GIANT_FILE_CHARS: usize = 250_000;
const ROW_PX: usize = 22;

const PREVIEW_CSS: &str = include_str!("embedded/preview.css");
const PREVIEW_BUNDLE: &str = include_str!("embedded/generated/preview.js");

/// Returns the embedded stylesheet shared by artifact and app renderers.
///
/// # Examples
///
/// ```
/// assert!(infra::html_renderer::preview_css().contains(".layout"));
/// ```
pub fn preview_css() -> &'static str {
    PREVIEW_CSS
}

/// Returns the embedded progressive-enhancement bundle shared by artifact and app renderers.
///
/// # Examples
///
/// ```
/// assert!(!infra::html_renderer::preview_bundle().is_empty());
/// ```
pub fn preview_bundle() -> &'static str {
    PREVIEW_BUNDLE
}

// ! Head boot: restore the saved theme and diff layout before paint to avoid a flash of the
// ! default palette / a unified→split flip. IIFE-wrapped so the locals never leak to global
// ! scope: a leaked var could clobber a minified bundle's single-letter globals.
const THEME_BOOT_JS: &str = "(function(){try{var d=document.documentElement.dataset;var t=localStorage.getItem('gtl-theme');if(t)d.theme=t;var l=localStorage.getItem('gtl-diff-layout');if(l==='split')d.diffLayout='split';else if(l==='unified')delete d.diffLayout;}catch(e){}})();";

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        push_escaped(&mut out, ch);
    }
    out
}

// ! Single source of the HTML escape mapping, char by char, so the intra-line span renderer
// ! can interleave `<span>` markers between escaped chars without re-escaping whole substrings.
fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(ch),
    }
}

pub fn slug(s: &str) -> String {
    let mut body = String::new();
    let mut last_dash = false;

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            body.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            body.push('-');
            last_dash = true;
        }
    }

    while body.starts_with('-') {
        body.remove(0);
    }
    while body.ends_with('-') {
        body.pop();
    }

    format!("f-{body}")
}

pub fn render_diff_lines(lines: &[String], owners: &LineOwners) -> String {
    use std::fmt::Write;

    let mut rows = String::with_capacity(lines.iter().map(String::len).sum::<usize>() * 2);

    for row in domain::diffs::derive_rows(lines, owners) {
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

// Body-only markup (the .layout block) shared by the single and tabbed views so the
// document chrome — one <style>/<script> — lives only at the top level. The Shelf is a
// 3-column grid: file tree (left), diff (center), commit shelf (right), titlebar + keybar
// spanning the full width. Per-commit [popover] elements live inside .layout so the
// per-layout JS scoping in preview.js finds them.
#[derive(Debug, Clone, Copy)]
enum RenderMode {
    Artifact,
    App(RenderOptions),
}

fn view_body(view: &View, mode: RenderMode) -> Markup {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_count = view.commits.len();
    let file_count = view.files.len();

    html! {
        div.layout.copy-ctx {
            header.titlebar {
                div.brand {
                    span.repo { "~/" b { (view.repo_name) } }
                    span.kind { (view.title) }
                }
                div.branchline {
                    span.ref-branch { (view.branch) }
                    span.arr { "→" }
                    span.ref-up { (view.upstream) }
                }
                div.spacer {}
                button type="button" class="foldall" title="Collapse/expand all files" { "Collapse all" }
                @if matches!(mode, RenderMode::Artifact) {
                    button type="button" class="layout-toggle" aria-pressed="false" title="Side-by-side / unified diff" { "Side by side" }
                    button type="button" class="view-toggle" aria-pressed="false" title="Show full-file diffs" { "Full file" }
                }
                button type="button" class="ctx-toggle active" aria-pressed="true" title="Prepend a commented “path, lines” header when copying code" { "+ context" }
                @if matches!(mode, RenderMode::Artifact) {
                    label.theme-control {
                        span { "theme" }
                        select class="theme-select" aria-label="Theme" {
                            option value="dark" { "dark" }
                            option value="light" { "light" }
                            option value="hearth" { "hearth" }
                        }
                    }
                }
            }
            aside.tree aria-label="Changed files tree" {
                div.search {
                    input type="text" class="filter" placeholder="Filter files…  /" aria-label="Filter files";
                }
                div.tree-head {
                    span { (view.commits_label) " · " (file_count) " file" (plural(file_count)) }
                }
                div.stats {
                    span.stat { b { (commit_count) } " commit" (plural(commit_count)) }
                    span.stat.add { "+" (total_add) }
                    span.stat.del { "−" (total_del) }
                }
                div.tree-body {}
            }
            main.main {
                (file_blocks(view, mode))
            }
            aside.shelf aria-label="Commits in range" {
                div.shelf-head {
                    h3 { (view.commits_label) }
                    p.hint { span.dot {} "click card = focus commit · hash = copy · hover = notes" }
                }
                (commit_rows(view))
            }
            footer.keybar {
                span.cmd-line {
                    (view.foot.cmd) " " span.dim { (view.foot.note) }
                }
                div.spacer {}
                span.key { kbd { "j" } " " kbd { "k" } " file" }
                span.key { kbd { "/" } " filter" }
                span.key { kbd { "alt+shift+c" } " fold all" }
            }
            (commit_popovers(view))
        }
    }
}

/// Builds one app-hosted diff view using only the requested layout and density variant.
///
/// The fragment retains the server-rendered file tree, commit shelf, popovers, and diff rows,
/// while leaving layout, density, and theme controls to the surrounding app shell.
///
/// # Examples
///
/// ```no_run
/// use domain::{diffs::View, viewer::RenderOptions};
/// use infra::html_renderer::build_view_fragment;
///
/// # fn load_view() -> View { todo!() }
/// let fragment = build_view_fragment(&load_view(), RenderOptions::DEFAULT);
/// assert!(fragment.into_string().contains("diff-unified diff-compact"));
/// ```
pub fn build_view_fragment(view: &View, options: RenderOptions) -> Markup {
    view_body(view, RenderMode::App(options))
}

// Native-popover bodies for commits that carry a body, emitted once per .layout (top-level
// [popover] elements escape the sidebar's scroll clip). The id is keyed to the sha so the
// shelf card's data-pop can resolve its popover within `root`.
fn commit_popovers(view: &View) -> Markup {
    html! {
        @for commit in &view.commits {
            @if !commit.body.trim().is_empty() {
                div id={ "pop-" (commit.sha) } popover {
                    div.pop-head {
                        span.sha { (commit.sha) }
                        @if !commit.date.is_empty() {
                            span.when { (commit.date) }
                        }
                    }
                    div.pop-sub { (commit.subject) }
                    div.pop-body { (commit.body.trim()) }
                }
            }
        }
    }
}

pub fn build_html(view: &View) -> String {
    let count = view.commits.len();
    html! {
        (DOCTYPE)
        html lang="en" data-theme=[view.theme.as_deref()] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                // ! page ships its own dark theme + switcher — tell Dark Reader to leave it alone.
                meta name="darkreader-lock";
                title { (view.repo_name) " — " (view.title) " · " (count) " commit" (plural(count)) }
                script { (PreEscaped(THEME_BOOT_JS)) }
                style { (PreEscaped(PREVIEW_CSS)) }
            }
            body {
                (view_body(view, RenderMode::Artifact))
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

/// The Maud-backed [`HtmlRenderer`](application::ports::HtmlRenderer) adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaudRenderer;

impl application::ports::HtmlRenderer for MaudRenderer {
    fn build_html(&self, view: &View) -> String {
        build_html(view)
    }
    fn build_tabbed_html(&self, title: &str, views: &[View]) -> String {
        build_tabbed_html(title, views)
    }
}

pub fn build_tabbed_html(title: &str, views: &[View]) -> String {
    // ? tab strip CSS stays inline; tab logic is in the bundle (tabbed.ts, guarded to no-op without
    // .tabs)
    const TABBED_CSS: &str = r"  .tabs{position:sticky;top:0;z-index:60;display:flex;gap:6px;align-items:center;overflow-x:auto;padding:10px 12px;background:var(--surface-2);border-bottom:1px solid var(--line)}
  .tab{flex:none;max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--ink-2);background:var(--surface);border:1px solid var(--line);border-radius:6px;padding:6px 10px;font:inherit;cursor:pointer}
  .tab:hover{color:var(--ink);border-color:var(--acc-line)} .tab.active{color:var(--acc);border-color:var(--acc-line)}
  .panel[hidden]{display:none}";

    let default_theme = views.first().and_then(|v| v.theme.as_deref());
    html! {
        (DOCTYPE)
        html lang="en" data-theme=[default_theme] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                // ! page ships its own dark theme + switcher — tell Dark Reader to leave it alone.
                meta name="darkreader-lock";
                title { (title) }
                script { (PreEscaped(THEME_BOOT_JS)) }
                style { (PreEscaped(PREVIEW_CSS)) }
                style { (PreEscaped(TABBED_CSS)) }
            }
            body {
                nav.tabs role="tablist" aria-label="Subrepo diffs" {
                    @for (index, view) in views.iter().enumerate() {
                        button class=(if index == 0 { "tab active" } else { "tab" })
                            id={ "tab-" (index) }
                            role="tab"
                            aria-selected=(if index == 0 { "true" } else { "false" })
                            aria-controls={ "panel-" (index) }
                            data-tab=(index) {
                            (view.repo_name)
                        }
                    }
                }
                @for (index, view) in views.iter().enumerate() {
                    section.panel id={ "panel-" (index) } role="tabpanel" aria-labelledby={ "tab-" (index) } hidden[index != 0] {
                        (view_body(view, RenderMode::Artifact))
                    }
                }
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

// ! Short shas are [0-9a-f]{9} (no HTML metacharacters), so no escaping is needed.
fn commit_attr(sha: Option<&String>) -> String {
    sha.map(|sha| format!(r#" data-commit="{sha}""#))
        .unwrap_or_default()
}

// ! Serialize a merge's brought-in commits for the focus set; non-merge / empty -> no attribute.
fn merge_members_attr(commit: &domain::diffs::Commit) -> Option<String> {
    (commit.is_merge() && !commit.members.is_empty()).then(|| commit.members.join(" "))
}

fn html_or_nbsp(raw: &str) -> String {
    let escaped = escape_html(raw);
    if escaped.is_empty() {
        "&nbsp;".to_string()
    } else {
        escaped
    }
}

// ! Inner content of a `<code>` cell: the bare body, or — for a tamed long line — the copy-safe
// ! `.code-text` span plus the expander button. Shared by the unified `code_cell` and the
// ! side-by-side `split_code` so the long-line taming lives in exactly one place.
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

// ! One side of a side-by-side row: a `<code>` carrying the change color (`side` is
// ! `sp-del`/`sp-add`/`sp-ctx`) and, for changed lines, the owning-commit attribute that
// ! drives the per-commit focus highlight. Long lines reuse `code_inner`'s taming; a paired
// ! changed line with `spans` gets its differing chars wrapped (the intra-line highlight).
fn split_code(
    raw: &str,
    long: Option<usize>,
    side: &str,
    commit: Option<&String>,
    spans: &[Span],
) -> String {
    let inner = if long.is_some() {
        code_inner(raw, long)
    } else if spans.is_empty() {
        html_or_nbsp(raw)
    } else {
        mark_spans(raw, spans)
    };
    let attr = commit_attr(commit);
    if long.is_some() {
        format!(r#"<code class="sp {side} long"{attr}>{inner}</code>"#)
    } else {
        format!(r#"<code class="sp {side}"{attr}>{inner}</code>"#)
    }
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

// Side-by-side (VS Code-style) counterpart of `render_diff_lines`: the same parsed diff laid
// out as old | new panes. Within a hunk, a run of deletions is paired index-wise with the
// following run of additions (the shorter side padded), context lines mirror on both panes,
// and meta/hunk headers span the full width. Same gutter-number tracking and owner tagging.
pub fn render_diff_split(lines: &[String], owners: &LineOwners) -> String {
    use std::fmt::Write;

    let mut rows = String::with_capacity(lines.iter().map(String::len).sum::<usize>() * 3);

    for row in domain::diffs::split_rows(&domain::diffs::derive_rows(lines, owners)) {
        match row {
            SplitRow::Meta { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-meta"><code>{}</code></div>"#,
                    html_or_nbsp(&text)
                );
            }
            SplitRow::Hunk { text } => {
                let _ = write!(
                    rows,
                    r#"<div class="dl dl-hunk"><code>{}</code></div>"#,
                    escape_html(&text)
                );
            }
            SplitRow::Context {
                old_no,
                new_no,
                text,
            } => {
                let long = long_line_len(&text);
                let _ = write!(
                    rows,
                    r#"<div class="dl"><span class="ln">{}</span>{}<span class="ln">{}</span>{}</div>"#,
                    old_no,
                    split_code(&text, long, "sp-ctx", None, &[]),
                    new_no,
                    split_code(&text, long, "sp-ctx", None, &[]),
                );
            }
            SplitRow::Pair { old, new } => {
                rows.push_str(r#"<div class="dl">"#);
                match &old {
                    Some(cell) => {
                        let _ = write!(
                            rows,
                            r#"<span class="ln">{}</span>{}"#,
                            cell.no,
                            split_code(
                                &cell.text,
                                long_line_len(&cell.text),
                                "sp-del",
                                cell.owner.as_ref(),
                                &cell.spans
                            ),
                        );
                    }
                    None => rows.push_str(SPLIT_PAD),
                }
                match &new {
                    Some(cell) => {
                        let _ = write!(
                            rows,
                            r#"<span class="ln">{}</span>{}"#,
                            cell.no,
                            split_code(
                                &cell.text,
                                long_line_len(&cell.text),
                                "sp-add",
                                cell.owner.as_ref(),
                                &cell.spans
                            ),
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

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn file_commits(file: &FileDiff) -> String {
    file.commits.join(" ")
}

// Right-hand commit shelf cards. The card filters files by commit; the hash tag copies its sha.
// A card with a body also gets a distinct `notes-ico` glyph + a `data-pop` pointer to its sibling
// [popover] (emitted by commit_popovers). The always-visible body `pre` of the old terminal layout
// is gone.
fn commit_rows(view: &View) -> Markup {
    if view.commits.is_empty() {
        return html! { div.empty { "no commits in range" } };
    }

    html! {
        @for commit in &view.commits {
            @let has_notes = !commit.body.trim().is_empty();
            div class=(if has_notes { "cline has" } else { "cline" })
                data-sha=(commit.sha)
                data-members=[merge_members_attr(commit)]
                data-pop=[has_notes.then(|| format!("pop-{}", commit.sha))]
                role="button" tabindex="0" title="focus this commit's changes" {
                span.bead aria-hidden="true" {}
                div.top {
                    button.sha type="button" title="copy hash" { (commit.sha) }
                    @if has_notes {
                        span.notes-ico aria-hidden="true" title="has extended notes" {}
                    }
                    @if commit.is_merge() && !commit.members.is_empty() {
                        span.merge-pill title="commits this merge brought in — focus to highlight them" {
                            "merge · " (commit.members.len())
                        }
                    }
                    @if !commit.date.is_empty() {
                        @if commit.iso.is_empty() {
                            time.when { (commit.date) }
                        } @else {
                            time.when datetime=(commit.iso) title=(commit.iso) { (commit.date) }
                        }
                    }
                }
                div.sub { (commit.subject) }
            }
        }
    }
}

fn file_blocks(view: &View, mode: RenderMode) -> Markup {
    if view.files.is_empty() {
        return html! { div.empty { "no file changes" } };
    }

    html! {
        @for file in &view.files {
            @let absolute = format!("{}/{}", view.repo_root, file.path);
            @let status = file.status();
            @let giant = file.lines.iter().map(String::len).sum::<usize>() > GIANT_FILE_CHARS;
            @let rows = file.lines.iter().filter(|l| !l.is_empty()).count();
            @let intrinsic = format!("contain-intrinsic-size:auto {}px", rows * ROW_PX);
            details open[!giant]
                id=(slug(&file.path))
                class=(format!("file {}", status.css_class()))
                data-path=(file.path)
                data-comment=(crate::comment_syntax::comment_leader(&file.path))
                data-commits=(file_commits(file))
                data-status=(status.key())
                data-status-code=(status.code())
                data-status-label=(status.label()) {
                summary {
                    span.path { (file.path) }
                    span class=(format!("status-badge {}", status.css_class())) title=(status.label()) aria-label=(status.label()) { (status.code()) }
                    span.copies {
                        button type="button" class="copy-button" data-copy-value=(file.path) data-copy-label="path" { "path" }
                        button type="button" class="copy-button" data-copy-value=(absolute) data-copy-label="abs" { "abs" }
                        // ! mode="code" carries no payload: the button reads its own file's
                        // ! already-rendered diff rows at click time (no per-file content dupe).
                        button type="button" class="copy-button" data-copy-mode="code" data-copy-label="code" { "code" }
                    }
                    span.filestat { span.a { "+" (file.added) } " " span.d { "−" (file.removed) } }
                }
                // ! Diff rows live in their own body so content-visibility virtualizes the
                // ! heavy content here while the summary stays sticky against `.main` (size
                // ! containment on `details.file` itself would trap the sticky in the box).
                // ! Artifacts ship four variants selected by <html> data attributes; app
                // ! fragments ship one visible variant selected by validated server options.
                div class=(if matches!(mode, RenderMode::Artifact) { "filebody" } else { "filebody single-variant" }) style=(intrinsic) {
                    (file_diff(file, mode))
                }
            }
        }
    }
}

fn file_diff(file: &FileDiff, mode: RenderMode) -> Markup {
    match mode {
        RenderMode::Artifact => html! {
            div class="diff diff-split diff-compact" { (PreEscaped(render_diff_split(&file.lines, &file.owners))) }
            div class="diff diff-unified diff-compact" { (PreEscaped(render_diff_lines(&file.lines, &file.owners))) }
            @if let Some(full_lines) = &file.full_lines {
                div class="diff diff-split diff-full" { (PreEscaped(render_diff_split(full_lines, &file.owners))) }
                div class="diff diff-unified diff-full" { (PreEscaped(render_diff_lines(full_lines, &file.owners))) }
            }
        },
        RenderMode::App(options) => {
            let lines = match options.density() {
                DiffDensity::Compact => &file.lines,
                DiffDensity::Full => file.full_lines.as_ref().unwrap_or(&file.lines),
            };
            match (options.layout(), options.density()) {
                (DiffLayout::Unified, DiffDensity::Compact) => html! {
                    div class="diff diff-unified diff-compact" { (PreEscaped(render_diff_lines(lines, &file.owners))) }
                },
                (DiffLayout::Split, DiffDensity::Compact) => html! {
                    div class="diff diff-split diff-compact" { (PreEscaped(render_diff_split(lines, &file.owners))) }
                },
                (DiffLayout::Unified, DiffDensity::Full) => html! {
                    div class="diff diff-unified diff-full" { (PreEscaped(render_diff_lines(lines, &file.owners))) }
                },
                (DiffLayout::Split, DiffDensity::Full) => html! {
                    div class="diff diff-split diff-full" { (PreEscaped(render_diff_split(lines, &file.owners))) }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        diffs::{Cmd, Commit, FileDiff, Foot, MAX_LINE_COLS, View},
        viewer::{DiffDensity, DiffLayout, RenderOptions},
    };

    use super::*;

    /// Returns true if `html` contains any http(s):// URL. Enforces the
    /// offline-artifact contract: nothing in the artifact may trigger a
    /// network load.
    fn has_disallowed_external_url(html: &str) -> bool {
        html.match_indices("://")
            .any(|(sep, _)| html[..sep].ends_with("http") || html[..sep].ends_with("https"))
    }

    #[test]
    fn escape_html_escapes_metacharacters() {
        assert_eq!(escape_html("&<>\""), "&amp;&lt;&gt;&quot;");
    }

    #[test]
    fn slug_normalizes_file_paths_to_anchor_ids() {
        assert_eq!(slug("src/a b.rs"), "f-src-a-b-rs");
    }

    #[test]
    fn slug_preserves_prefix_when_normalized_body_is_empty() {
        assert_eq!(slug("---"), "f-");
    }

    #[test]
    fn render_diff_lines_classifies_rows_and_tracks_gutter_numbers() {
        let html = render_diff_lines(
            &[
                "index 111..222 100644".to_string(),
                "@@ -3,2 +7,2 @@".to_string(),
                " keep".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
            &LineOwners::default(),
        );

        assert!(html.contains(r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>index 111..222 100644</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>@@ -3,2 +7,2 @@</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">3</span><span class="ln">7</span><code> keep</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-del"><span class="ln">4</span><span class="ln"></span><code>-old</code></div>"#));
        assert!(html.contains(r#"<div class="dl dl-add"><span class="ln"></span><span class="ln">8</span><code>+new</code></div>"#));
    }

    #[test]
    fn render_diff_lines_tags_rows_with_owning_commit() {
        let mut owners = domain::diffs::LineOwners::default();
        owners.added.insert(8, "abc123def".to_string());
        owners.deleted.insert(4, "fff000aaa".to_string());
        let html = render_diff_lines(
            &[
                "@@ -3,2 +7,2 @@".to_string(),
                " keep".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
            &owners,
        );
        assert!(html.contains(r#"<div class="dl dl-del" data-commit="fff000aaa">"#));
        assert!(html.contains(r#"<div class="dl dl-add" data-commit="abc123def">"#));
        // context rows carry no owner attribute
        assert!(html.contains(r#"<div class="dl dl-ctx"><span"#));
    }

    #[test]
    fn render_diff_split_pairs_changes_and_mirrors_context() {
        let html = render_diff_split(
            &[
                "index 111..222 100644".to_string(),
                "@@ -3,2 +7,2 @@".to_string(),
                " keep".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
            &LineOwners::default(),
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
    fn render_diff_split_pads_the_shorter_change_run() {
        let html = render_diff_split(
            &[
                "@@ -1,2 +1,1 @@".to_string(),
                "-a".to_string(),
                "-b".to_string(),
                "+c".to_string(),
            ],
            &LineOwners::default(),
        );

        // first deletion pairs with the lone addition
        assert!(html.contains(r#"<div class="dl"><span class="ln">1</span><code class="sp sp-del">-a</code><span class="ln">1</span><code class="sp sp-add">+c</code></div>"#));
        // the surplus deletion gets a blank, shaded filler on the new pane
        assert!(html.contains(r#"<div class="dl"><span class="ln">2</span><code class="sp sp-del">-b</code><span class="ln"></span><code class="sp sp-pad"></code></div>"#));
    }

    #[test]
    fn render_diff_split_tags_sides_with_owning_commit() {
        let mut owners = domain::diffs::LineOwners::default();
        owners.added.insert(8, "abc123def".to_string());
        owners.deleted.insert(4, "fff000aaa".to_string());
        let html = render_diff_split(
            &[
                "@@ -3,2 +7,2 @@".to_string(),
                " keep".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
            &owners,
        );

        assert!(html.contains(r#"<code class="sp sp-del" data-commit="fff000aaa">-old</code>"#));
        assert!(html.contains(r#"<code class="sp sp-add" data-commit="abc123def">+new</code>"#));
        // context sides carry no owner attribute
        assert!(html.contains(r#"<code class="sp sp-ctx"> keep</code>"#));
    }

    #[test]
    fn render_diff_split_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_diff_split(&["@@ -0,0 +1 @@".to_string(), long], &LineOwners::default());
        assert!(html.contains(r#"<code class="sp sp-add long">"#));
        assert!(html.contains(r#"<span class="code-text">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            MAX_LINE_COLS + 5
        )));
    }

    #[test]
    fn render_diff_split_marks_intra_line_word_changes_on_both_panes() {
        let html = render_diff_split(
            &[
                "@@ -1,1 +1,1 @@".to_string(),
                "-let x = 1;".to_string(),
                "+let x = 2;".to_string(),
            ],
            &LineOwners::default(),
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
    fn render_diff_split_leaves_fully_rewritten_pairs_unmarked() {
        let html = render_diff_split(
            &[
                "@@ -1,1 +1,1 @@".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
            &LineOwners::default(),
        );

        // no shared run -> the line color already conveys the change, no intra-line marks
        assert!(!html.contains("ciw"));
        assert!(html.contains(r#"<code class="sp sp-del">-old</code>"#));
        assert!(html.contains(r#"<code class="sp sp-add">+new</code>"#));
    }

    #[test]
    fn render_diff_split_skips_intra_line_marks_on_unpaired_lines() {
        // a lone addition (no deletion to pair with) is wholly new -> no intra-line marks
        let html = render_diff_split(
            &["@@ -0,0 +1 @@".to_string(), "+brandnew".to_string()],
            &LineOwners::default(),
        );

        assert!(!html.contains("ciw"));
    }

    #[test]
    fn render_diff_split_skips_intra_line_marks_on_long_lines() {
        let del = format!("-{}x", "a".repeat(MAX_LINE_COLS + 5));
        let add = format!("+{}y", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_diff_split(
            &["@@ -1,1 +1,1 @@".to_string(), del, add],
            &LineOwners::default(),
        );

        // long lines are tamed, not word-diffed (a word-diff over base64 would be pointless)
        assert!(!html.contains("ciw"));
        assert!(html.contains(r#"<code class="sp sp-del long">"#));
        assert!(html.contains(r#"<code class="sp sp-add long">"#));
    }

    #[test]
    fn render_diff_split_marks_escape_user_controlled_chars() {
        let html = render_diff_split(
            &[
                "@@ -1,1 +1,1 @@".to_string(),
                "-a<b>&1".to_string(),
                "-".to_string(),
                "+a<b>&2".to_string(),
            ],
            &LineOwners::default(),
        );

        // the marked char stays escaped inside the span; no raw `<b>` leaks
        assert!(html.contains(r#"a&lt;b&gt;&amp;<span class="ciw">1</span>"#));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn preview_css_styles_intra_line_word_spans() {
        assert!(PREVIEW_CSS.contains(".diff-split .sp-del .ciw{"));
        assert!(PREVIEW_CSS.contains(".diff-split .sp-add .ciw{"));
    }

    #[test]
    fn preview_css_drives_split_default_and_breakpoint_fallback() {
        // base: every pane hidden until a rule reveals exactly one
        assert!(PREVIEW_CSS.contains(".filebody .diff{display:none}"));
        // unified is the default pane above the breakpoint (no data-diff-layout attr)
        assert!(PREVIEW_CSS.contains("@media (min-width:1025px)"));
        assert!(PREVIEW_CSS.contains(
            r#"html:not([data-diff-layout="split"]):not([data-diff-full="on"]) .diff-unified.diff-compact{display:block}"#
        ));
        // split is shown when the attr flips
        assert!(PREVIEW_CSS.contains(
            r#"html[data-diff-layout="split"]:not([data-diff-full="on"]) .diff-split.diff-compact{display:block}"#
        ));
        // narrow screens force the combined pane and hide the layout toggle
        assert!(PREVIEW_CSS.contains(".layout-toggle{display:none}"));
        // four-column split grid + the per-pane change colors
        assert!(PREVIEW_CSS.contains(
            ".diff-split .dl{grid-template-columns:44px minmax(0,1fr) 44px minmax(0,1fr)"
        ));
        assert!(PREVIEW_CSS.contains(".diff-split .sp-add{background:var(--add-bg)"));
        assert!(PREVIEW_CSS.contains(".diff-split .sp-del{background:var(--del-bg)"));
    }

    #[test]
    fn preview_css_tames_long_lines_without_wrap() {
        assert!(PREVIEW_CSS.contains(".dl-long .code-text,.diff-split code.long .code-text{flex:1;min-width:0;white-space:pre"));
        assert!(PREVIEW_CSS.contains(
            ".dl-long.expanded .code-text,.diff-split code.long.expanded .code-text{overflow-x:auto"
        ));
    }

    #[test]
    fn render_diff_lines_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_diff_lines(&["@@ -0,0 +1 @@".to_string(), long], &LineOwners::default());
        assert!(html.contains(r#"class="dl dl-add dl-long""#));
        assert!(html.contains(r#"<span class="code-text">"#));
        assert!(html.contains(&format!(
            r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#,
            MAX_LINE_COLS + 5
        )));
    }

    #[test]
    fn render_diff_lines_leaves_normal_lines_untamed() {
        let html = render_diff_lines(
            &["@@ -0,0 +1 @@".to_string(), "+short".to_string()],
            &LineOwners::default(),
        );
        assert!(!html.contains("dl-long"));
        assert!(!html.contains("code-text"));
    }

    #[test]
    fn render_diff_lines_does_not_treat_malformed_headers_as_hunks() {
        let html = render_diff_lines(&["@@ -1, +2 @@".to_string()], &LineOwners::default());

        assert!(!html.contains("dl-hunk"));
        assert!(html.contains(r#"<div class="dl dl-ctx"><span class="ln">0</span><span class="ln">0</span><code>@@ -1, +2 @@</code></div>"#));
    }

    #[test]
    fn build_html_renders_offline_document_with_core_diff_data() {
        let view = View {
            repo_name: "api".to_string(),
            repo_root: "/home/user/api".to_string(),
            branch: "main".to_string(),
            upstream: "origin/main".to_string(),
            commits: vec![Commit {
                sha: "abc123def".to_string(),
                subject: "feat: thing".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: Vec::new(),
                members: Vec::new(),
            }],
            files: vec![FileDiff {
                path: "src/a b.rs".to_string(),
                added: 2,
                removed: 1,
                lines: vec![
                    "@@ -1 +1,2 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                ],
                full_lines: Some(vec![
                    "@@ -1,4 +1,5 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                    " middle".to_string(),
                    " end".to_string(),
                ]),
                commits: vec!["abc123def".to_string()],
                owners: domain::diffs::LineOwners::default(),
            }],
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: "origin/main..HEAD".to_string(),
                trail: String::new(),
            },
            commits_label: "# commits".to_string(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".to_string(),
                note: "# read-only preview".to_string(),
            },
            theme: None,
        };

        let html = build_html(&view);

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(
            !has_disallowed_external_url(&html),
            "artifact must not reference any external http(s) resource"
        );
        assert!(html.contains("api"));
        assert!(html.contains("origin/main..HEAD"));
        assert!(html.contains("src/a b.rs"));
        assert!(html.contains(r#"<span class="a">+2</span>"#));
        assert!(html.contains(r#"<span class="d">−1</span>"#));
    }

    #[test]
    fn build_html_guards_shelf_structural_contract() {
        let html = build_html(&sample_view());

        // title reflects repo, view, and commit count
        assert!(html.contains("<title>api — diff · 1 commit</title>"));

        // three theme palettes: default :root (dark) + light + hearth, amber removed
        assert!(html.contains(":root{"));
        assert!(html.contains(r#":root[data-theme="light"]"#));
        assert!(html.contains(r#":root[data-theme="hearth"]"#));
        assert!(!html.contains(r#":root[data-theme="amber"]"#));

        // perf + offline-theming guards survive the redesign
        assert!(html.contains("content-visibility:auto"));
        assert!(html.contains("@media print"));

        // native controls replace Lit custom elements
        assert!(html.contains(r#"class="theme-select""#));
        assert!(html.contains(r#"class="copy-button""#));

        // engine diff classes are styled (render_diff_lines emits these, untouched)
        assert!(html.contains(".dl-add"));
        assert!(html.contains(".dl-del"));

        // native popover machinery + Shelf landmarks
        assert!(html.contains("[popover]"));
        assert!(html.contains(r#"<aside class="tree""#));
        assert!(html.contains(r#"<aside class="shelf""#));
        assert!(html.contains(r#"<footer class="keybar""#));

        // commit-filter feature: files carry data-commits
        assert!(html.contains(r#"data-commits="abc123def""#));

        // commit card body filters by commit; the hash tag copies the hash.
        assert!(html.contains(r#"title="focus this commit's changes""#));
        assert!(html.contains(r#"<button class="sha" type="button" title="copy hash""#));
        // the timeline bead is a visual marker, not a separate click target.
        assert!(html.contains(r#"<span class="bead" aria-hidden="true"></span>"#));
        assert!(!html.contains(r#"<button class="bead""#));
        // notes are flagged by a distinct, non-emoji notes indicator (not the bead)
        assert!(html.contains(r#"<span class="notes-ico" aria-hidden="true""#));

        // offline: no external resource loads (CDN scripts, stylesheets, fetches)
        assert!(
            !has_disallowed_external_url(&html),
            "artifact must not reference any external http(s) resource"
        );
    }

    #[test]
    fn commit_shelf_click_contract_focuses_card_and_copies_hash_tag() {
        // ! JS behavior: sha-guard predicate (isShaTarget) covered by Vitest wheel.test.ts.
        // ! copyText + stopPropagation wiring is event-listener-only and not extracted.
        let html = build_html(&sample_view());

        assert!(html.contains(r#"title="focus this commit's changes""#));
        assert!(html.contains(r#"<button class="sha" type="button" title="copy hash""#));
        assert!(html.contains(r#"<span class="bead" aria-hidden="true"></span>"#));
        assert!(!html.contains(r#"<button class="bead""#));
        assert!(!html.contains(r#"title="click to copy hash""#));
    }

    #[test]
    fn commit_focus_highlight_is_wired() {
        // ! JS behavior: resolveActiveSet (toggle/member-set) covered by Vitest preview.test.ts.
        // ! owned-row DOM mutation is event-listener-only and not extracted.
        assert!(PREVIEW_CSS.contains(".commit-focus .diff-unified .dl.owned{opacity:1}"));
        assert!(PREVIEW_CSS.contains(".commit-focus .diff-split .sp.owned{opacity:1"));
        assert!(PREVIEW_CSS.contains("inset 3px 0 0 var(--acc)"));
        assert!(PREVIEW_CSS.contains("prefers-reduced-motion"));
    }

    #[test]
    fn build_html_escapes_user_controlled_values() {
        let mut view = sample_view();
        view.repo_name = "a&b<repo>\"".to_string();
        view.branch = "main<script>".to_string();
        view.upstream = "origin/feat\"x".to_string();
        view.commits[0].subject = "feat: a&b<x>".to_string();
        view.files[0].path = "src/<x>&\".rs".to_string();
        view.files[0].lines = vec![
            "@@ -0,0 +1 @@".to_string(),
            "+<script>x</script>".to_string(),
        ];

        let html = build_html(&view);

        assert!(html.contains("a&amp;b&lt;repo&gt;&quot;"));
        assert!(html.contains("main&lt;script&gt;"));
        assert!(html.contains("origin/feat&quot;x"));
        assert!(html.contains("feat: a&amp;b&lt;x&gt;"));
        assert!(html.contains("src/&lt;x&gt;&amp;&quot;.rs"));
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("+&lt;script&gt;x&lt;/script&gt;"));
    }

    #[test]
    fn build_tabbed_html_wraps_each_repo_view_in_a_tab() {
        let mut api = sample_view();
        api.repo_name = "api".to_string();
        let mut web = sample_view();
        web.repo_name = "web".to_string();

        let html = build_tabbed_html("subrepo diff", &[api, web]);

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches(r#"<section class="panel""#).count(), 2);
        assert_eq!(html.matches(r#"role="tabpanel""#).count(), 2);
        assert!(html.contains("api"));
        assert!(html.contains("web"));
        assert!(!html.contains("<iframe"));
    }

    #[test]
    fn build_html_theme_select_is_offline() {
        let html = build_html(&sample_view());

        assert!(html.contains(r#"class="theme-select""#));
        // no external resource loads (CDN scripts, stylesheets, fetches)
        assert!(
            !has_disallowed_external_url(&html),
            "artifact must not reference any external http(s) resource"
        );
    }

    #[test]
    fn build_html_file_block_carries_copy_buttons() {
        let html = build_html(&sample_view());

        // relative path copy button: data-copy-value attribute
        assert!(html.contains(r#"data-copy-value="src/a b.rs" data-copy-label="path""#));
        // absolute path copy button: repo_root + "/" + path (POSIX join)
        assert!(
            html.contains(r#"data-copy-value="/home/user/api/src/a b.rs" data-copy-label="abs""#)
        );
        // copy-code-without-markers reads its own file's rendered rows at click time
        assert!(html.contains(r#"data-copy-mode="code""#));
    }

    #[test]
    fn build_html_wires_copy_context_toggle_and_per_file_comment_leader() {
        let html = build_html(&sample_view());

        // context-on by default: the root carries the class the copy-button reads at click time
        assert!(html.contains(r#"class="layout copy-ctx""#));
        // header toggle starts active/pressed
        assert!(html.contains(r#"class="ctx-toggle active" aria-pressed="true""#));
        // the .rs file advertises the // comment leader for the pasteable header
        assert!(html.contains(r#"data-comment="//""#));
    }

    #[test]
    fn build_html_defaults_to_unified_layout_and_ships_all_four_panes() {
        let html = build_html(&sample_view());

        // both header toggles: unified is the default, full file is not
        assert!(html.contains(r#"<html lang="en""#));
        assert!(!html.contains(r#"<html lang="en" data-diff-layout="#));
        assert!(html.contains(r#"class="layout-toggle" aria-pressed="false""#));
        assert!(html.contains(r#"class="view-toggle" aria-pressed="false""#));
        // all four diff renderings ship; CSS reveals one (no `hidden` plumbing)
        assert!(html.contains(r#"class="diff diff-split diff-compact""#));
        assert!(html.contains(r#"class="diff diff-unified diff-compact""#));
        assert!(html.contains(r#"class="diff diff-split diff-full""#));
        assert!(html.contains(r#"class="diff diff-unified diff-full""#));
        // visibility is CSS-driven now; the diff blocks carry no `hidden` attribute
        assert!(!html.contains(r#"diff-full" hidden"#));
        assert!(!html.contains(r#"diff-compact" hidden"#));
    }

    #[test]
    fn app_fragment_emits_only_the_requested_variant() {
        let html = build_view_fragment(
            &sample_view(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-split diff-full""#));
        assert!(!html.contains("diff-unified"));
        assert!(!html.contains("diff-compact"));
    }

    #[test]
    fn app_fragment_omits_artifact_owned_controls_and_theme_boot() {
        let html = build_view_fragment(&sample_view(), RenderOptions::DEFAULT).into_string();

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
        assert!(!html.contains("localStorage"));
        assert!(html.contains(r#"<aside class="tree""#));
        assert!(html.contains(r#"<aside class="shelf""#));
        assert!(html.contains(r#"<div id="pop-abc123def" popover>"#));
        assert!(preview_css().contains(".filebody.single-variant .diff{display:block}"));
    }

    #[test]
    fn app_fragment_full_density_falls_back_to_compact_source_lines() {
        let mut view = sample_view();
        view.files[0].full_lines = None;

        let html = build_view_fragment(
            &view,
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-unified diff-full""#));
        assert!(html.contains("+extra"));
        assert!(!html.contains("diff-compact"));
    }

    #[test]
    fn raw_documents_still_emit_all_four_variants_and_artifact_controls() {
        let view = sample_view();
        let documents = [build_html(&view), build_tabbed_html("diffs", &[view])];

        for html in documents {
            for class in [
                "diff-split diff-compact",
                "diff-unified diff-compact",
                "diff-split diff-full",
                "diff-unified diff-full",
            ] {
                assert!(html.contains(class), "missing {class}");
            }
            assert!(html.contains(r#"class="layout-toggle""#));
            assert!(html.contains(r#"class="view-toggle""#));
            assert!(html.contains(r#"class="theme-select""#));
            assert!(html.contains(THEME_BOOT_JS));
        }
    }

    #[test]
    fn public_preview_assets_are_the_embedded_offline_payloads() {
        assert_eq!(preview_css(), PREVIEW_CSS);
        assert_eq!(preview_bundle(), PREVIEW_BUNDLE);
        assert!(!has_disallowed_external_url(preview_css()));
        assert!(!has_disallowed_external_url(preview_bundle()));
    }

    #[test]
    fn layout_boot_script_preserves_saved_split_preference() {
        assert!(THEME_BOOT_JS.contains("if(l==='split')d.diffLayout='split';"));
    }

    #[test]
    fn preview_css_wraps_diff_code_inside_fixed_line_number_gutters() {
        assert!(PREVIEW_CSS.contains(".diff{overflow-x:hidden"));
        assert!(PREVIEW_CSS.contains("grid-template-columns:44px 44px minmax(0,1fr)"));
        assert!(PREVIEW_CSS.contains(".dl{display:grid"));
        assert!(PREVIEW_CSS.contains("white-space:normal"));
        assert!(PREVIEW_CSS.contains(".dl code{"));
        assert!(PREVIEW_CSS.contains("white-space:pre-wrap"));
        assert!(PREVIEW_CSS.contains("overflow-wrap:anywhere"));
        assert!(PREVIEW_CSS.contains("min-width:0"));
    }

    #[test]
    fn build_html_marks_file_status_for_sidebar_tree() {
        let mut view = sample_view();
        view.files = vec![
            FileDiff {
                path: "src/new.rs".to_string(),
                added: 1,
                removed: 0,
                lines: vec![
                    "new file mode 100644".to_string(),
                    "--- /dev/null".to_string(),
                    "+++ b/src/new.rs".to_string(),
                    "@@ -0,0 +1 @@".to_string(),
                    "+hello".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: domain::diffs::LineOwners::default(),
            },
            FileDiff {
                path: "src/gone.rs".to_string(),
                added: 0,
                removed: 1,
                lines: vec![
                    "deleted file mode 100644".to_string(),
                    "--- a/src/gone.rs".to_string(),
                    "+++ /dev/null".to_string(),
                    "@@ -1 +0,0 @@".to_string(),
                    "-bye".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: domain::diffs::LineOwners::default(),
            },
            FileDiff {
                path: "src/new-name.rs".to_string(),
                added: 0,
                removed: 0,
                lines: vec![
                    "similarity index 100%".to_string(),
                    "rename from src/old-name.rs".to_string(),
                    "rename to src/new-name.rs".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: domain::diffs::LineOwners::default(),
            },
        ];

        let html = build_html(&view);

        assert!(html.contains(r#"data-status="added""#));
        assert!(html.contains(r#"data-status-label="Added file""#));
        assert!(html.contains(r#"class="file status-added""#));
        assert!(html.contains(r#"data-status="deleted""#));
        assert!(html.contains(r#"data-status-label="Deleted file""#));
        assert!(html.contains(r#"class="file status-deleted""#));
        assert!(html.contains(r#"data-status="renamed""#));
        assert!(html.contains(r#"data-status-label="Renamed file""#));
        assert!(html.contains(r#"class="file status-renamed""#));
        assert!(html.contains("tstatus"));
    }

    #[test]
    fn file_status_indicators_stay_compact_trailing_and_discreet() {
        // ! JS behavior: buildFileLeaf (li class, [name,status] child order, no icon) covered
        // ! by Vitest wheel.test.ts. Horizontal-wheel scroll math covered by computeWheelScroll
        // there.
        assert!(PREVIEW_CSS.contains(".tfile.status-added>.tlabel"));
        assert!(PREVIEW_CSS.contains(".tfile.status-deleted>.tlabel"));
        assert!(
            PREVIEW_CSS.contains(".tlabel{display:flex;align-items:center;gap:5px;padding:2px 5px")
        );
        assert!(PREVIEW_CSS.contains("gap:7px;padding:7px 10px;font-size:12.5px"));
    }

    #[test]
    fn preview_css_uses_responsive_sidebar_columns() {
        assert!(PREVIEW_CSS.contains("--tree-col:262px"));
        assert!(PREVIEW_CSS.contains("--shelf-col:252px"));
        assert!(
            PREVIEW_CSS
                .contains("grid-template-columns:var(--tree-col) minmax(0,1fr) var(--shelf-col)")
        );
        assert!(PREVIEW_CSS.contains("@media (min-width:1600px) and (min-height:900px)"));
        assert!(PREVIEW_CSS.contains("--tree-col:320px"));
        assert!(PREVIEW_CSS.contains("--shelf-col:304px"));
        assert!(PREVIEW_CSS.contains("@media (max-width:1280px)"));
        assert!(PREVIEW_CSS.contains("--tree-col:220px"));
        assert!(PREVIEW_CSS.contains("--shelf-col:210px"));
        assert!(PREVIEW_CSS.contains("@media (max-width:1024px)"));
        assert!(PREVIEW_CSS.contains("--side-display:none"));
        assert!(PREVIEW_CSS.contains(".tdir>ul .tfile>.tlabel{padding-left:8px}"));
        assert!(PREVIEW_CSS.contains(".tree{grid-column:1;grid-row:2"));
        assert!(PREVIEW_CSS.contains(".main{grid-column:2;grid-row:2"));
        assert!(PREVIEW_CSS.contains(".shelf{grid-column:3;grid-row:2"));
        assert!(!PREVIEW_CSS.contains(".keybar{display:none}"));
    }

    #[test]
    fn build_html_copy_button_absolute_path_escapes_user_controlled_values() {
        let mut view = sample_view();
        view.repo_root = "/tmp/<r>".to_string();
        view.files[0].path = "src/<x>&\".rs".to_string();

        let html = build_html(&view);

        // both relative and absolute data-copy-value attributes stay Maud-escaped
        assert!(
            html.contains(
                r#"data-copy-value="src/&lt;x&gt;&amp;&quot;.rs" data-copy-label="path""#
            )
        );
        assert!(html.contains(
            r#"data-copy-value="/tmp/&lt;r&gt;/src/&lt;x&gt;&amp;&quot;.rs" data-copy-label="abs""#
        ));
        assert!(!html.contains("<x>"));
    }

    #[test]
    fn build_html_ships_content_visibility_perf_rule() {
        let html = build_html(&sample_view());

        assert!(html.contains("content-visibility:auto"));
    }

    #[test]
    fn commit_rows_marks_a_merge_card_with_members_and_pill() {
        let mut view = sample_view();
        view.commits = vec![
            Commit {
                sha: "merge1234".to_string(),
                subject: "Merge branch 'sub'".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string(), "p2bbbbbbb".to_string()],
                members: vec!["aaa111aaa".to_string(), "bbb222bbb".to_string()],
            },
            Commit {
                sha: "plain5678".to_string(),
                subject: "feat: x".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string()],
                members: Vec::new(),
            },
        ];

        let html = build_html(&view);

        assert!(html.contains(r#"data-members="aaa111aaa bbb222bbb""#));
        assert!(html.contains("merge · 2"));
        // exactly one card is a merge: no pill / no data-members leaks onto the plain card
        assert_eq!(html.matches("merge · ").count(), 1);
        assert_eq!(html.matches("data-members=").count(), 1);
        // the styling shipped
        assert!(PREVIEW_CSS.contains(".cline .merge-pill{"));
    }

    #[test]
    fn commit_rows_skips_pill_for_a_merge_with_no_in_range_members() {
        let mut view = sample_view();
        view.commits = vec![Commit {
            sha: "merge1234".to_string(),
            subject: "Merge branch 'main'".to_string(),
            body: String::new(),
            date: String::new(),
            iso: String::new(),
            parents: vec!["p1aaaaaaa".to_string(), "p2bbbbbbb".to_string()],
            members: Vec::new(), // base-bounded walk found nothing in range
        }];

        let html = build_html(&view);

        assert!(!html.contains("merge · "));
        assert!(!html.contains("data-members="));
    }

    #[test]
    fn file_blocks_collapses_giant_files() {
        let mut view = sample_view();
        let huge = "+".to_string() + &"x".repeat(GIANT_FILE_CHARS);
        view.files[0].lines = vec!["@@ -0,0 +1 @@".to_string(), huge];
        view.files[0].full_lines = None;
        let html = build_html(&view);
        // the giant file's <details> renders WITHOUT `open` (Maud emits `open` before `id`)
        assert!(html.contains(r#"<details id="f-src-a-b-rs""#));
        assert!(!html.contains(r#"<details open id="f-src-a-b-rs""#));
    }

    #[test]
    fn file_blocks_emits_per_file_intrinsic_size() {
        let html = build_html(&sample_view());
        // sample file renders 4 rows -> 4 * ROW_PX
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }

    #[test]
    fn content_visibility_stays_in_css_not_inline_so_print_override_wins() {
        let html = build_html(&sample_view());
        // content-visibility must NOT be inline (an inline style out-specifies the @media print
        // override)
        assert!(!html.contains(r#"style="content-visibility"#));
        // the old inline pattern must not appear (CSS rule contains this substring but not as an
        // inline style)
        assert!(!html.contains(r#"style="content-visibility:auto;contain-intrinsic-size"#));
        // it still lives in the stylesheet, and the print override is present
        assert!(html.contains("content-visibility:auto")); // base CSS rule
        assert!(html.contains("content-visibility:visible")); // @media print override
        // per-file intrinsic-size is still emitted inline
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }

    fn sample_view() -> View {
        View {
            repo_name: "api".to_string(),
            repo_root: "/home/user/api".to_string(),
            branch: "main".to_string(),
            upstream: "origin/main".to_string(),
            commits: vec![Commit {
                sha: "abc123def".to_string(),
                subject: "feat: thing".to_string(),
                body: "extended notes".to_string(),
                date: String::new(),
                iso: String::new(),
                parents: Vec::new(),
                members: Vec::new(),
            }],
            files: vec![FileDiff {
                path: "src/a b.rs".to_string(),
                added: 2,
                removed: 1,
                lines: vec![
                    "@@ -1 +1,2 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                ],
                full_lines: Some(vec![
                    "@@ -1,4 +1,5 @@".to_string(),
                    "-old".to_string(),
                    "+new".to_string(),
                    "+extra".to_string(),
                    " middle".to_string(),
                    " end".to_string(),
                ]),
                commits: vec!["abc123def".to_string()],
                owners: domain::diffs::LineOwners::default(),
            }],
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: "origin/main..HEAD".to_string(),
                trail: String::new(),
            },
            commits_label: "# commits".to_string(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".to_string(),
                note: "# read-only preview".to_string(),
            },
            theme: None,
        }
    }
}
