//! Top titlebar: repo/branch identity, the exclusion chip, fold/copy-context
//! toggles.

use gtl_application::diffs::View;
use gtl_models::diffs::AppliedExclusions;
use maud::{Markup, html};

use crate::text::plural;

const CONTROL_CLASSES: &str = "cursor-pointer rounded-sm border border-line-2 bg-surface-2 px-2.5 py-1.5 text-[12px] text-ink-2 [font:inherit] hover:border-acc-line hover:text-ink print:hidden!";

// ! `.foldall` and `.ctx-toggle` remain enhancer hooks for their toggled states.
pub(super) fn titlebar(view: &View, mobile_controls_target: Option<&str>) -> Markup {
    html! {
        header class="titlebar [grid-column:1/4] flex items-center gap-4 border-b border-line bg-surface px-5 py-3 tablet:flex-wrap tablet:gap-2.5 tablet:px-3 tablet:py-2.5 mobile:gap-1.5 mobile:px-2 mobile:py-2 print:border-[#bbb] print:bg-[#f2f2f2]" {
            div class="flex items-baseline gap-2 text-[18px] font-semibold tracking-[-0.01em] mobile:text-[15px]" {
                span { "~/" b class="font-bold text-acc" { (view.repo_name) } }
                span class="self-center rounded-sm border border-acc-line bg-acc-soft px-2 py-0.5 text-[12px] font-medium text-acc" { (view.title) }
            }
            div class="branchline flex items-center gap-1.5 text-[12.5px] text-ink-2 tablet:order-3 tablet:w-full" {
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
            @if let Some(target) = mobile_controls_target {
                (super::mobile_controls::view_navigation(target, true))
            }
            button type="button" class={ "foldall " (CONTROL_CLASSES) " mobile:hidden" } title="Collapse/expand all files" { "Collapse all" }
            button type="button" class={ "ctx-toggle active " (CONTROL_CLASSES) " [&.active]:border-acc-line [&.active]:bg-acc-soft [&.active]:text-ink mobile:hidden" } aria-pressed="true" title="Prepend a commented “path, lines” header when copying code" { "+ context" }
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
    use gtl_application::viewer::{RenderOptions, ViewerTabId};
    use gtl_models::diffs::AppliedExclusions;

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

        let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
        let fragment = view_fragment(&view, RenderOptions::DEFAULT, tab_id).into_string();

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
