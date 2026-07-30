//! Self-contained offline documents: one `file://`-ready HTML page per diff
//! view (or one tab-stripped page for several), with the stylesheet, boot
//! script, and enhancement bundle inlined so nothing loads from the network.

mod tabbed;

use application::{diffs::View, viewer::RenderOptions};
use maud::{DOCTYPE, PreEscaped, html};
pub use tabbed::build_tabbed_html;

use crate::{
    assets::{PREVIEW_BUNDLE, preview_css},
    layout::{Surface, view_body},
    text::plural,
};

// ! Head boot: restore the saved theme before paint to avoid a palette flash. Built by
// ! `deno task build` from frontend/boot/ (Vite lib IIFE), so locals never leak into the
// ! minified bundle's global scope.
const THEME_BOOT_JS: &str = include_str!("embedded/generated/boot.js");

pub fn build_html(view: &View, options: RenderOptions, theme: Option<&str>) -> String {
    let count = view.commits.len();
    html! {
        (DOCTYPE)
        html lang="en" data-theme=[theme] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                // ! The page ships its own palette, so Dark Reader must leave it alone.
                meta name="darkreader-lock";
                title { (view.repo_name) " — " (view.title) " · " (count) " commit" (plural(count)) }
                script { (PreEscaped(THEME_BOOT_JS)) }
                style { (PreEscaped(preview_css())) }
            }
            body {
                (view_body(view, options, Surface::Artifact { view_index: 0 }))
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

#[cfg(test)]
mod tests {
    use application::{
        diffs::{Cmd, FileDiff, Foot, LineOwners, View},
        viewer::{RenderOptions, ViewerTabId},
    };
    use domain::diffs::Commit;

    use super::THEME_BOOT_JS;
    use crate::{
        build_html, build_tabbed_html,
        fixtures::{has_disallowed_external_url, sample_view},
        view_fragment,
    };

    #[test]
    fn artifact_and_app_fragment_keep_their_surface_presentation_distinct() {
        let view = sample_view();
        let options = RenderOptions::DEFAULT;
        let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
        let fragment = view_fragment(&view, options, tab_id).into_string();
        let html = build_html(&view, options, None);

        assert!(fragment.contains("files/open"));
        assert!(!html.contains("files/open"));
        assert!(!fragment.contains("content-visibility"));
        assert!(html.contains("content-visibility"));
    }

    #[test]
    fn raw_layout_has_no_presentation_controls() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
    }

    #[test]
    fn raw_documents_render_shared_mobile_controls_with_per_view_targets() {
        let raw = build_html(&sample_view(), RenderOptions::DEFAULT, None);
        let tabbed = build_tabbed_html(
            "diffs",
            &[sample_view(), sample_view()],
            RenderOptions::DEFAULT,
            None,
        );

        assert!(raw.contains(r#"aria-label="View settings""#));
        assert!(raw.contains(r#"popovertarget="preview-controls-popover-0""#));
        assert!(raw.contains(r#"id="preview-controls-popover-0" class="preview-mobile-controls "#));
        assert!(raw.contains(r#"data-preview-action="fold-all""#));
        assert!(raw.contains(r#"data-preview-action="toggle-context""#));

        for index in 0..2 {
            let target = format!("preview-controls-popover-{index}");
            assert_eq!(
                tabbed
                    .matches(&format!(r#"popovertarget="{target}""#))
                    .count(),
                2
            );
            assert_eq!(tabbed.matches(&format!(r#"id="{target}""#)).count(), 1);
        }
    }

    #[test]
    fn build_html_renders_offline_document_with_core_diff_data() {
        let view = View {
            exclusions: None,
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
                owners: LineOwners::default(),
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
        };

        let html = build_html(&view, RenderOptions::DEFAULT, None);

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
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        // title reflects repo, view, and commit count
        assert!(html.contains("<title>api — diff · 1 commit</title>"));

        // theme palettes: default :root,[data-theme=dark] plus every other
        // palette (assets.rs asserts every `Theme::VARIANTS` entry has a
        // token block; this spot-checks a couple survive into the document).
        assert!(html.contains(":root,[data-theme=dark]{"));
        assert!(html.contains(r"[data-theme=light]"));
        assert!(html.contains(r"[data-theme=hearth]"));
        assert!(!html.contains(r"[data-theme=amber]"));

        // perf + offline-theming guards survive the redesign
        assert!(html.contains("content-visibility:auto"));
        assert!(html.contains("@media print"));

        // native copy controls replace Lit custom elements
        assert!(html.contains(r#"class="copy-button "#));

        // engine diff classes are styled (render_diff_lines emits these, untouched)
        assert!(html.contains(".dl-add"));
        assert!(html.contains(".dl-del"));

        // native popover machinery + Shelf landmarks
        assert!(html.contains(" popover>"));
        assert!(html.contains("[&amp;::backdrop]:bg-transparent"));
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains(r#"<aside class="shelf"#));
        assert!(html.contains(r#"<footer class="keybar"#));

        // commit-filter feature: files carry data-commits
        assert!(html.contains(r#"data-commits="abc123def""#));

        // commit card body filters by commit; the hash tag copies the hash.
        assert!(html.contains(r#"title="focus this commit's changes""#));
        assert!(html.contains(r#"<button class="sha "#));
        assert!(html.contains(r#"type="button" title="copy hash""#));
        // the timeline bead is a visual marker, not a separate click target.
        assert!(html.contains(r#"<span class="bead "#));
        assert!(html.contains(r#"aria-hidden="true"></span>"#));
        assert!(!html.contains(r#"<button class="bead""#));
        // notes are flagged by a distinct, non-emoji notes indicator (not the bead)
        assert!(html.contains(r#"<span class="notes-ico "#));

        // offline: no external resource loads (CDN scripts, stylesheets, fetches)
        assert!(
            !has_disallowed_external_url(&html),
            "artifact must not reference any external http(s) resource"
        );
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

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains("a&amp;b&lt;repo&gt;&quot;"));
        assert!(html.contains("main&lt;script&gt;"));
        assert!(html.contains("origin/feat&quot;x"));
        assert!(html.contains("feat: a&amp;b&lt;x&gt;"));
        assert!(html.contains("src/&lt;x&gt;&amp;&quot;.rs"));
        // content is escaped even when interleaved with syntax token spans:
        // no raw script element can exist, and the code cell's entity-decoded
        // text still carries the exact user content.
        assert!(
            !html.contains("<script>x</script>"),
            "raw script tag leaked: {html}"
        );
        let fragment = scraper::Html::parse_fragment(&html);
        let code = scraper::Selector::parse("code").unwrap();
        assert!(
            fragment.select(&code).any(|cell| cell
                .text()
                .collect::<String>()
                .contains("+<script>x</script>")),
            "escaped diff content lost: {html}"
        );
    }

    #[test]
    fn build_tabbed_html_wraps_each_repo_view_in_a_tab() {
        let mut api = sample_view();
        api.repo_name = "api".to_string();
        let mut web = sample_view();
        web.repo_name = "web".to_string();

        let html = build_tabbed_html("subrepo diff", &[api, web], RenderOptions::DEFAULT, None);

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches(r#"<section class="panel "#).count(), 2);
        assert_eq!(html.matches(r#"role="tabpanel""#).count(), 2);
        assert!(html.contains("api"));
        assert!(html.contains("web"));
        assert!(!html.contains("<iframe"));
    }

    #[test]
    fn build_html_is_offline() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        // no external resource loads (CDN scripts, stylesheets, fetches)
        assert!(
            !has_disallowed_external_url(&html),
            "artifact must not reference any external http(s) resource"
        );
    }

    #[test]
    fn offline_artifact_omits_configured_editor_actions() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(!html.contains("files/open"));
        assert!(!html.contains("Open in IDE"));
    }

    #[test]
    fn build_html_defaults_to_one_unified_compact_variant() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"<html lang="en""#));
        assert!(html.contains(r#"class="diff diff-unified diff-compact "#));
        assert!(!html.contains(r#"class="diff diff-split"#));
        assert!(!html.contains(r#"class="diff diff-unified diff-full"#));
    }

    #[test]
    fn raw_documents_emit_only_the_requested_variant_without_presentation_controls() {
        let view = sample_view();
        let options = RenderOptions::new(
            application::viewer::DiffLayout::Split,
            application::viewer::DiffDensity::Full,
        );
        let documents = [
            build_html(&view, options, None),
            build_tabbed_html("diffs", &[view], options, None),
        ];

        for html in documents {
            assert_eq!(
                html.matches(r#"class="diff diff-split diff-full "#).count(),
                1
            );
            assert!(!html.contains(r#"class="diff diff-unified"#));
            assert!(!html.contains(r#"class="diff diff-split diff-compact"#));
            assert!(!html.contains(r#"class="layout-toggle""#));
            assert!(!html.contains(r#"class="view-toggle""#));
            assert!(!html.contains(r#"class="theme-select""#));
            assert!(html.contains(THEME_BOOT_JS));
        }
    }

    #[test]
    fn theme_boot_script_reads_only_the_saved_theme() {
        assert!(THEME_BOOT_JS.contains("gtl-theme"));
        assert!(!THEME_BOOT_JS.contains("gtl-diff-layout"));
    }

    #[test]
    fn theme_boot_script_stays_within_its_pre_paint_budget() {
        assert!(
            THEME_BOOT_JS.len() <= 160,
            "the boot script blocks the first paint; it is {} bytes",
            THEME_BOOT_JS.len()
        );
    }
}
