//! Self-contained offline documents: one `file://`-ready HTML page per diff
//! view (or one tab-stripped page for several), with the stylesheet, boot
//! script, and enhancement bundle inlined so nothing loads from the network.

mod tabbed;

use application::diffs::View;
use maud::{DOCTYPE, PreEscaped, html};
pub use tabbed::build_tabbed_html;

use crate::{
    assets::{PREVIEW_BUNDLE, preview_css},
    layout::{Surface, view_body},
    text::plural,
};

// ! Head boot: restore the saved theme and diff layout before paint to avoid a flash of the
// ! default palette / a unified→split flip. IIFE-wrapped so the locals never leak to global
// ! scope: a leaked var could clobber a minified bundle's single-letter globals.
// ! Authored in frontend/inline/theme-boot.ts and shipped verbatim (build copies the bytes).
const THEME_BOOT_JS: &str = include_str!("../embedded/generated/boot.js");

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
                style { (PreEscaped(preview_css())) }
            }
            body {
                (view_body(view, Surface::Artifact))
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}

#[cfg(test)]
mod tests {
    use application::diffs::{Cmd, FileDiff, Foot, LineOwners, View};
    use domain::diffs::Commit;

    use super::THEME_BOOT_JS;
    use crate::{
        build_html, build_tabbed_html,
        fixtures::{has_disallowed_external_url, sample_view},
    };

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
        assert!(html.contains(r":root[data-theme=light]"));
        assert!(html.contains(r":root[data-theme=hearth]"));
        assert!(!html.contains(r":root[data-theme=amber]"));

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
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains(r#"<aside class="shelf"#));
        assert!(html.contains(r#"<footer class="keybar"#));

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
    fn layout_boot_script_preserves_saved_split_preference() {
        assert!(THEME_BOOT_JS.contains("if(l==='split')d.diffLayout='split';"));
    }
}
