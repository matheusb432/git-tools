//! Top titlebar: repo/branch identity, the exclusion chip, fold/copy-context
//! toggles, and (artifact surface only) the layout/density toggles and theme
//! select.

use application::diffs::View;
use domain::diffs::AppliedExclusions;
use maud::{Markup, html};

use super::Surface;
use crate::text::plural;

pub(super) fn titlebar(view: &View, surface: Surface) -> Markup {
    html! {
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
            @if let Some(excluded) = &view.exclusions {
                span.excl-chip title=(exclusion_tooltip(excluded)) {
                    (excluded.hidden_paths.len())
                    " file" (plural(excluded.hidden_paths.len()))
                    " hidden · " (excluded.extensions_label())
                }
                // TODO: add button to enable file exclusion modification here. should open a dialog.
            }
            div.spacer {}
            button type="button" class="foldall" title="Collapse/expand all files" { "Collapse all" }
            @if matches!(surface, Surface::Artifact) {
                button type="button" class="layout-toggle" aria-pressed="false" title="Side-by-side / unified diff" { "Side by side" }
                button type="button" class="view-toggle" aria-pressed="false" title="Show full-file diffs" { "Full file" }
            }
            button type="button" class="ctx-toggle active" aria-pressed="true" title="Prepend a commented “path, lines” header when copying code" { "+ context" }
            @if matches!(surface, Surface::Artifact) {
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

        let html = build_html(&view);

        assert!(
            html.contains(r#"<span class="excl-chip""#),
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
        assert!(!build_html(&sample_view()).contains(r#"<span class="excl-chip""#));
    }

    #[test]
    fn view_fragment_carries_the_exclusion_chip_into_the_app_shell() {
        let mut view = sample_view();
        view.exclusions = Some(applied_exclusions());

        let fragment = view_fragment(&view, RenderOptions::DEFAULT).into_string();

        assert!(fragment.contains(r#"<span class="excl-chip""#));
    }

    #[test]
    fn build_html_escapes_user_controlled_exclusion_values() {
        let mut view = sample_view();
        view.exclusions = Some(AppliedExclusions {
            extensions: vec!["md".to_string()],
            hidden_paths: vec!["a&b<script>.md".to_string()],
        });

        let html = build_html(&view);

        assert!(html.contains("a&amp;b&lt;script&gt;.md"));
        assert!(!html.contains("a&b<script>.md"));
    }
}
