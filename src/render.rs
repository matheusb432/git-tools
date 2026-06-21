use crate::model::{FileDiff, LineOwners, View};
use maud::{DOCTYPE, Markup, PreEscaped, html};

const MAX_LINE_COLS: usize = 2000;
const GIANT_FILE_CHARS: usize = 250_000;
const ROW_PX: usize = 22;

const PREVIEW_CSS: &str = include_str!("assets/preview.css");
const PREVIEW_BUNDLE: &str = include_str!("assets/generated/preview.js");

// ! Head boot: restore the saved theme before paint to avoid a flash of the default palette.
// ! IIFE-wrapped so `t` never leaks to global scope: a leaked var could clobber a minified
// ! bundle's single-letter globals.
const THEME_BOOT_JS: &str = "(function(){try{var t=localStorage.getItem('gtl-theme');if(t)document.documentElement.dataset.theme=t;}catch(e){}})();";

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut rows = String::with_capacity(lines.iter().map(String::len).sum::<usize>() * 2);

    for raw in lines {
        if raw.is_empty() {
            continue;
        }

        if is_meta_line(raw) {
            let _ = write!(
                rows,
                r#"<div class="dl dl-meta"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                html_or_nbsp(raw)
            );
            continue;
        }

        if let Some((old_start, new_start)) = hunk_starts(raw) {
            old_no = old_start;
            new_no = new_start;
            let _ = write!(
                rows,
                r#"<div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>{}</code></div>"#,
                escape_html(raw)
            );
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            let long = long_len(raw);
            let _ = write!(
                rows,
                r#"<div class="dl dl-add{}"{}><span class="ln"></span><span class="ln">{}</span>{}</div>"#,
                if long.is_some() { " dl-long" } else { "" },
                commit_attr(owners.added.get(&new_no)),
                new_no,
                code_cell(raw, long),
            );
            new_no += 1;
        } else if raw.starts_with('-') && !raw.starts_with("---") {
            let long = long_len(raw);
            let _ = write!(
                rows,
                r#"<div class="dl dl-del{}"{}><span class="ln">{}</span><span class="ln"></span>{}</div>"#,
                if long.is_some() { " dl-long" } else { "" },
                commit_attr(owners.deleted.get(&old_no)),
                old_no,
                code_cell(raw, long),
            );
            old_no += 1;
        } else {
            let long = long_len(raw);
            let _ = write!(
                rows,
                r#"<div class="dl dl-ctx{}"><span class="ln">{}</span><span class="ln">{}</span>{}</div>"#,
                if long.is_some() { " dl-long" } else { "" },
                old_no,
                new_no,
                code_cell(raw, long),
            );
            old_no += 1;
            new_no += 1;
        }
    }

    rows
}

// Body-only markup (the .layout block) shared by the single and tabbed views so the
// document chrome — one <style>/<script> — lives only at the top level. The Shelf is a
// 3-column grid: file tree (left), diff (center), commit shelf (right), titlebar + keybar
// spanning the full width. Per-commit [popover] elements live inside .layout so the
// per-layout JS scoping in preview.js finds them.
fn view_body(view: &View) -> Markup {
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
                button type="button" class="view-toggle" aria-pressed="false" title="Show full-file diffs" { "Full file" }
                button type="button" class="ctx-toggle active" aria-pressed="true" title="Prepend a commented “path, lines” header when copying code" { "+ context" }
                theme-switch {}
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
                (file_blocks(view))
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
                (view_body(view))
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

pub fn build_tabbed_html(title: &str, views: &[View]) -> String {
    // ? tab strip CSS stays inline; tab logic is in the bundle (tabbed.ts, guarded to no-op without .tabs)
    const TABBED_CSS: &str = r#"  .tabs{position:sticky;top:0;z-index:60;display:flex;gap:6px;align-items:center;overflow-x:auto;padding:10px 12px;background:var(--surface-2);border-bottom:1px solid var(--line)}
  .tab{flex:none;max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--ink-2);background:var(--surface);border:1px solid var(--line);border-radius:6px;padding:6px 10px;font:inherit;cursor:pointer}
  .tab:hover{color:var(--ink);border-color:var(--acc-line)} .tab.active{color:var(--acc);border-color:var(--acc-line)}
  .panel[hidden]{display:none}"#;

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
                        (view_body(view))
                    }
                }
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

fn is_meta_line(raw: &str) -> bool {
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

// ! Short shas are [0-9a-f]{9} (no HTML metacharacters), so no escaping is needed.
fn commit_attr(sha: Option<&String>) -> String {
    sha.map(|sha| format!(r#" data-commit="{sha}""#))
        .unwrap_or_default()
}

// ! Serialize a merge's brought-in commits for the focus set; non-merge / empty -> no attribute.
fn merge_members_attr(commit: &crate::model::Commit) -> Option<String> {
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

// ! Char length excluding the leading diff marker; `Some(len)` only when the line is long enough
// ! to need taming (see code_cell). Computed once per row and shared by the row class and the
// ! code cell so the O(n) char count isn't walked twice on the very lines the freeze fix targets.
fn long_len(raw: &str) -> Option<usize> {
    let marker = matches!(raw.as_bytes().first(), Some(b'+' | b'-' | b' ')) as usize;
    let len = raw.chars().count().saturating_sub(marker);
    (len > MAX_LINE_COLS).then_some(len)
}

// ! Long lines (e.g. base64 data URIs) would force char-by-char wrap layout and freeze the
// ! page. Tame them: full text stays in `.code-text` (copy-safe) but renders clipped/no-wrap,
// ! with an expander that reveals horizontal scroll. `long` is the precomputed Some(len).
fn code_cell(raw: &str, long: Option<usize>) -> String {
    let body = html_or_nbsp(raw);
    let Some(len) = long else {
        return format!("<code>{body}</code>");
    };
    format!(
        r#"<code class="long"><span class="code-text">{body}</span><button class="ln-more" type="button" aria-expanded="false">⋯ {len} chars</button></code>"#
    )
}

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

fn file_blocks(view: &View) -> Markup {
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
                        copy-button value=(file.path) label="path" {}
                        copy-button value=(absolute) label="abs" {}
                        // ! mode="code" carries no payload: the button reads its own file's
                        // ! already-rendered diff rows at click time (no per-file content dupe).
                        copy-button mode="code" label="code" {}
                    }
                    span.filestat { span.a { "+" (file.added) } " " span.d { "−" (file.removed) } }
                }
                // ! Diff rows live in their own body so content-visibility virtualizes the
                // ! heavy content here while the summary stays sticky against `.main` (size
                // ! containment on `details.file` itself would trap the sticky in the box).
                div class="filebody" style=(intrinsic) {
                    div class="diff diff-compact" { (PreEscaped(render_diff_lines(&file.lines, &file.owners))) }
                    @if let Some(full_lines) = &file.full_lines {
                        div class="diff diff-full" hidden { (PreEscaped(render_diff_lines(full_lines, &file.owners))) }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cmd, Commit, FileDiff, Foot, View};

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
        let mut owners = crate::model::LineOwners::default();
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
    fn preview_css_tames_long_lines_without_wrap() {
        assert!(PREVIEW_CSS.contains(".dl-long .code-text{flex:1;min-width:0;white-space:pre"));
        assert!(PREVIEW_CSS.contains(".dl-long.expanded .code-text{overflow-x:auto"));
    }

    #[test]
    fn render_diff_lines_tames_overlong_lines() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let html = render_diff_lines(&["@@ -0,0 +1 @@".to_string(), long], &LineOwners::default());
        assert!(html.contains(r#"class="dl dl-add dl-long""#));
        assert!(html.contains(r#"<span class="code-text">"#));
        assert!(html.contains(&format!(r#"<button class="ln-more" type="button" aria-expanded="false">⋯ {} chars</button>"#, MAX_LINE_COLS + 5)));
    }

    #[test]
    fn render_diff_lines_leaves_normal_lines_untamed() {
        let html = render_diff_lines(&["@@ -0,0 +1 @@".to_string(), "+short".to_string()], &LineOwners::default());
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
                owners: crate::model::LineOwners::default(),
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
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
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

        // vendored Lit components stay in place
        assert!(html.contains("<theme-switch"));
        assert!(html.contains("<copy-button"));

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

        // offline: no network resources anywhere
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn commit_shelf_click_contract_focuses_card_and_copies_hash_tag() {
        // ! JS behavior: sha-guard predicate (isShaTarget) covered by bun wheel.test.ts.
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
        // ! JS behavior: resolveActiveSet (toggle/member-set) covered by bun preview.test.ts.
        // ! owned-row DOM mutation is event-listener-only and not extracted.
        assert!(PREVIEW_CSS.contains(".commit-focus .dl.owned{opacity:1}"));
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
    fn build_html_boots_lit_and_theme_switch_offline() {
        let html = build_html(&sample_view());

        assert!(html.contains("customElements"));
        assert!(html.contains("<theme-switch"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn build_html_file_block_carries_copy_buttons() {
        let html = build_html(&sample_view());

        // relative path copy button
        assert!(html.contains(r#"<copy-button value="src/a b.rs" label="path">"#));
        // absolute path copy button: repo_root + "/" + path (POSIX join)
        assert!(html.contains(r#"<copy-button value="/home/user/api/src/a b.rs" label="abs">"#));
        // copy-code-without-markers reads its own file's rendered rows at click time
        assert!(html.contains(r#"<copy-button mode="code""#));
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
    fn build_html_renders_global_full_file_toggle_and_alternate_panes() {
        let html = build_html(&sample_view());

        assert!(html.contains(r#"class="view-toggle""#));
        assert!(html.contains(r#"aria-pressed="false""#));
        assert!(html.contains(r#"class="diff diff-compact""#));
        assert!(html.contains(r#"class="diff diff-full" hidden"#));
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
                owners: crate::model::LineOwners::default(),
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
                owners: crate::model::LineOwners::default(),
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
                owners: crate::model::LineOwners::default(),
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
        // ! by bun wheel.test.ts. Horizontal-wheel scroll math covered by computeWheelScroll there.
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

        // both relative and absolute copy-button values stay Maud-escaped
        assert!(html.contains(r#"value="src/&lt;x&gt;&amp;&quot;.rs" label="path""#));
        assert!(html.contains(r#"value="/tmp/&lt;r&gt;/src/&lt;x&gt;&amp;&quot;.rs" label="abs""#));
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
        // content-visibility must NOT be inline (an inline style out-specifies the @media print override)
        assert!(!html.contains(r#"style="content-visibility"#));
        // the old inline pattern must not appear (CSS rule contains this substring but not as an inline style)
        assert!(!html.contains(r#"style="content-visibility:auto;contain-intrinsic-size"#));
        // it still lives in the stylesheet, and the print override is present
        assert!(html.contains("content-visibility:auto"));     // base CSS rule
        assert!(html.contains("content-visibility:visible"));  // @media print override
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
                owners: crate::model::LineOwners::default(),
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
