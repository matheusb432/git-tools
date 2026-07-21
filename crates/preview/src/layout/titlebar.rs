//! Top titlebar: repo/branch identity, the exclusion chip, fold/copy-context
//! toggles.

use application::diffs::View;
use domain::diffs::AppliedExclusions;
use maud::{Markup, html};

use crate::text::plural;

// ! `.titlebar` and `.branchline` stay as class anchors for the narrow-screen overrides in
// ! styles/responsive.css; their base appearance is the utility classes here. `.foldall` and
// ! `.ctx-toggle` are enhancer hooks whose JS-toggled states keep component styling in
// ! styles/layout.css.
pub(super) fn titlebar(view: &View) -> Markup {
    html! {
        header class="titlebar [grid-column:1/4] flex items-center gap-4 border-b border-line bg-surface px-5 py-[13px]" {
            div class="flex items-baseline gap-[9px] text-[18px] font-semibold tracking-[-0.01em]" {
                span { "~/" b class="font-bold text-acc" { (view.repo_name) } }
                span class="self-center rounded-sm border border-acc-line bg-acc-soft px-2 py-0.5 text-[12px] font-medium text-acc" { (view.title) }
            }
            div class="branchline flex items-center gap-[7px] text-[12.5px] text-ink-2" {
                span class="text-acc" { (view.branch) }
                span class="text-ink-3" { "→" }
                span class="text-ink-3" { (view.upstream) }
            }
            @if let Some(excluded) = &view.exclusions {
                span class="excl-chip flex-none cursor-help whitespace-nowrap rounded-sm border border-del-line bg-del-bg px-2 py-0.5 text-[12px] font-semibold text-del-ink"
                    title=(exclusion_tooltip(excluded)) {
                    (excluded.hidden_paths.len())
                    " file" (plural(excluded.hidden_paths.len()))
                    " hidden · " (excluded.extensions_label())
                }
                // TODO: add button to enable file exclusion modification here. should open a dialog.
            }
            div class="flex-1" {}
            button type="button" class="foldall" title="Collapse/expand all files" { "Collapse all" }
            button type="button" class="ctx-toggle active" aria-pressed="true" title="Prepend a commented “path, lines” header when copying code" { "+ context" }
        }
    }
}

// ! The chip's hover tooltip: names the config source, then every hidden path,
// ! so a "missing" file is one hover away from its explanation.
fn exclusion_tooltip(excluded: &AppliedExclusions) -> String {
    let mut tooltip = String::from("Hidden by git-tools config [diff.exclude]:");
    for path in &excluded.hidden_paths {
        tooltip.push('\n');
        tooltip.push_str(path);
    }
    tooltip
}

#[cfg(test)]
mod tests {
    use application::viewer::RenderOptions;
    use domain::diffs::AppliedExclusions;

    use crate::{
        build_html,
        fixtures::{applied_exclusions, sample_view},
        view_fragment,
    };

    #[test]
    fn build_html_shows_the_exclusion_chip_when_files_were_hidden() {
        let mut view = sample_view();
        view.exclusions = Some(applied_exclusions());

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(
            html.contains(r#"<span class="excl-chip"#),
            "chip missing: {html}"
        );
        assert!(html.contains("2 files hidden · lock, md"));
        assert!(
            html.contains("Hidden by git-tools config [diff.exclude]:\ndocs/plan.md\nCargo.lock"),
            "tooltip must list every hidden path"
        );
    }

    #[test]
    fn build_html_omits_the_exclusion_chip_without_hidden_files() {
        // ? the class name still appears once — in the inlined stylesheet
        assert!(
            !build_html(&sample_view(), RenderOptions::DEFAULT, None)
                .contains(r#"<span class="excl-chip"#)
        );
    }

    #[test]
    fn view_fragment_carries_the_exclusion_chip_into_the_app_shell() {
        let mut view = sample_view();
        view.exclusions = Some(applied_exclusions());

        let fragment = view_fragment(&view, RenderOptions::DEFAULT).into_string();

        assert!(fragment.contains(r#"<span class="excl-chip"#));
    }

    #[test]
    fn build_html_escapes_user_controlled_exclusion_values() {
        let mut view = sample_view();
        view.exclusions = Some(AppliedExclusions {
            extensions: vec!["md".to_string()],
            hidden_paths: vec!["a&b<script>.md".to_string()],
        });

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains("a&amp;b&lt;script&gt;.md"));
        assert!(!html.contains("a&b<script>.md"));
    }
}
