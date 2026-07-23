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
                (view_body(view, options, Surface::Artifact))
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
        viewer::RenderOptions,
    };
    use domain::diffs::Commit;

    use super::THEME_BOOT_JS;
    use crate::{
        build_html, build_tabbed_html,
        fixtures::{has_disallowed_external_url, sample_view},
        view_fragment,
    };

    #[test]
    fn artifact_and_app_fragment_differ_only_in_filebody_presentation() {
        let view = sample_view();
        let options = RenderOptions::DEFAULT;
        let fragment = view_fragment(&view, options).into_string();
        let html = build_html(&view, options, None);
        let artifact_as_app = html
            .replace(
                "filebody single-variant [content-visibility:auto] overflow-hidden rounded-b-panel print:block! print:[content-visibility:visible] print:overflow-visible",
                "filebody single-variant overflow-hidden rounded-b-panel",
            )
            .replace(r#" style="contain-intrinsic-size:auto 88px""#, "");

        assert_eq!(artifact_as_app.matches(&fragment).count(), 1);
    }

    #[test]
    fn raw_layout_has_no_presentation_controls() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
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

        // three theme palettes: default :root (dark) + light + hearth, amber removed
        assert!(html.contains(":root{"));
        assert!(html.contains(r":root[data-theme=light]"));
        assert!(html.contains(r":root[data-theme=hearth]"));
        assert!(!html.contains(r":root[data-theme=amber]"));

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
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("+&lt;script&gt;x&lt;/script&gt;"));
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
}
