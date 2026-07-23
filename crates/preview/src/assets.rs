//! Embedded stylesheet and progressive-enhancement bundle shared by the
//! artifact documents and the app shell. Both are build outputs committed
//! under `embedded/generated/`: the stylesheet compiles from `styles/` through
//! Tailwind, the bundle from `frontend/` through vite (`deno task build`), and
//! the xtask drift gate keeps the committed bytes in sync with their sources.

const RAW_PREVIEW_CSS: &str = include_str!("embedded/generated/preview.css");
pub(crate) const PREVIEW_BUNDLE: &str = include_str!("embedded/generated/preview.js");

// ! The compiler banner comment carries an https:// URL; artifacts ban `http(s)://`
// ! outright so the offline contract stays a plain substring scan.
/// Strips the leading Tailwind banner comment from a compiled stylesheet.
///
/// # Examples
///
/// ```
/// let css = "/*! tailwindcss */:root{--x:1}";
/// assert_eq!(preview::strip_stylesheet_banner(css), ":root{--x:1}");
/// ```
pub fn strip_stylesheet_banner(css: &str) -> &str {
    css.strip_prefix("/*!")
        .and_then(|rest| rest.split_once("*/"))
        .map_or(css, |(_, tail)| tail.trim_start())
}

/// Returns the embedded stylesheet shared by artifact and app renderers.
///
/// # Examples
///
/// ```
/// assert!(preview::preview_css().contains("content-visibility:auto"));
/// ```
pub fn preview_css() -> &'static str {
    strip_stylesheet_banner(RAW_PREVIEW_CSS)
}

/// Returns the embedded progressive-enhancement bundle shared by artifact and app renderers.
///
/// # Examples
///
/// ```
/// assert!(!preview::preview_bundle().is_empty());
/// ```
pub fn preview_bundle() -> &'static str {
    PREVIEW_BUNDLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::has_disallowed_external_url;

    fn assert_selector_declaration(css: &str, selector: &str, declaration: &str) {
        assert!(
            css.split('}')
                .any(|rule| rule.contains(selector) && rule.contains(declaration)),
            "missing `{declaration}` on selector `{selector}`"
        );
    }

    #[test]
    fn public_preview_assets_are_the_embedded_offline_payloads() {
        let css = preview_css();
        assert!(!css.starts_with("/*!"), "compiler banner must be stripped");
        assert!(!css.contains("/*!"));
        assert!(css.contains(":root{"));
        assert_eq!(preview_bundle(), PREVIEW_BUNDLE);
        assert!(!has_disallowed_external_url(css));
        assert!(!has_disallowed_external_url(preview_bundle()));
    }

    #[test]
    fn preflight_reset_stays_layered_below_utilities() {
        // ! An unlayered reset beats every layered rule, stripping the 1px width that
        // ! border utilities apply; the base layer must also precede utilities.
        let css = preview_css();
        assert!(css.contains("*,:before,:after{box-sizing:border-box;border:0 solid}"));
        let base = css.find("@layer base").expect("base layer present");
        let utilities = css
            .find("@layer utilities")
            .expect("utilities layer present");
        assert!(base < utilities);
    }

    #[test]
    fn preview_css_emits_theme_scale_utilities() {
        // ! The entry imports utilities.css without Tailwind's default theme, so any
        // ! scale utility compiles to nothing unless theme-map.css defines its token.
        let css = preview_css();
        assert!(css.contains(".px-5{padding-inline:1.25rem}"));
        assert!(css.contains(".gap-4{gap:1rem}"));
        assert!(css.contains(".rounded-sm{border-radius:var(--r-sm)}"));
        assert!(css.contains(".font-semibold{"));
        assert!(css.contains(".border-add-line{border-color:var(--add-line)}"));
        assert!(css.contains(".border-del-line{border-color:var(--del-line)}"));
    }

    #[test]
    fn preview_css_confines_motion_to_the_compositor_only_toast_fade() {
        let css = preview_css();

        // The toast fade is the sole motion: it animates only compositor-only
        // properties, so rows never repaint. No keyframes, no smooth scroll, and
        // no shorthand that could smuggle box-shadow or filter.
        assert!(css.contains("transition-property:opacity,scale"));
        assert!(!css.contains("transition:"));
        assert!(!css.contains("animation:"));
        assert!(!css.contains("animation-name:"));
        assert!(!css.contains("scroll-behavior:smooth"));

        const PREFIX: &str = "transition-property:";
        for (index, _) in css.match_indices(PREFIX) {
            let value = css[index + PREFIX.len()..]
                .split([';', '}'])
                .next()
                .unwrap_or_default();
            assert!(
                value.split(',').all(|property| matches!(
                    property,
                    "opacity" | "scale" | "translate" | "transform" | "none"
                )),
                "repaint-inducing transition-property in preview CSS: {value}"
            );
        }
    }

    #[test]
    fn preview_css_styles_intra_line_word_spans() {
        let css = preview_css();
        assert!(css.contains(".diff-split .sp-del .ciw{"));
        assert!(css.contains(".diff-split .sp-add .ciw{"));
    }

    #[test]
    fn preview_css_emits_split_tracks_and_narrow_stack() {
        let css = preview_css();
        assert!(css.contains("grid-template-columns:44px minmax(0,1fr) 44px minmax(0,1fr)"));
        assert!(css.contains("@media (max-width:1024px)"));
        assert!(!css.contains("@media not all and (min-width:1024px)"));
        assert!(css.contains("grid-template-columns:44px minmax(0,1fr)"));
        assert!(css.contains("background-color:var(--add-bg)"));
        assert!(css.contains("background-color:var(--del-bg)"));
        assert!(!css.contains("data-diff-full"));
    }

    #[test]
    fn preview_css_keeps_mid_width_layout_rules_out_of_narrow_viewports() {
        let css = preview_css();

        assert!(css.contains("@media (min-width:1025px) and (max-width:1280px)"));
        assert!(css.contains("grid-template-columns:220px minmax(0,1fr) 210px"));
        assert!(css.contains("@media (max-width:1024px)"));
        assert!(css.contains("grid-template-columns:0 minmax(0,1fr) 0"));
    }

    #[test]
    fn preview_css_keeps_narrow_split_metadata_rows_full_width() {
        let css = preview_css();
        let narrow = css
            .split_once("@media (max-width:1024px)")
            .and_then(|(_, tail)| tail.split_once("}@media"))
            .map(|(block, _)| block)
            .expect("narrow media block");
        let stacked_rows = narrow
            .find(".diff-split .dl{grid-template-columns:44px minmax(0,1fr)")
            .expect("stacked split rows");
        let metadata_rows = narrow
            .find(".diff-split :is(.dl-meta,.dl-hunk){grid-template-columns:minmax(0,1fr)")
            .expect("full-width split metadata rows");

        assert!(stacked_rows < metadata_rows);
    }

    #[test]
    fn preview_css_tames_long_lines_without_wrap() {
        let css = preview_css();
        let collapsed = ":is(.dl-long .code-text,.diff-split code.long .code-text)";
        let expanded =
            ":is(.dl-long.expanded .code-text,.diff-split code.long.expanded .code-text)";

        assert_selector_declaration(css, collapsed, "white-space:pre");
        assert_selector_declaration(css, collapsed, "overflow:hidden");
        assert_selector_declaration(css, collapsed, "text-overflow:ellipsis");
        assert_selector_declaration(css, expanded, "overflow-x:auto");
        assert_selector_declaration(css, expanded, "text-overflow:clip");
    }

    #[test]
    fn preview_css_wraps_diff_code_inside_fixed_line_number_gutters() {
        let css = preview_css();
        assert!(css.contains("grid-template-columns:44px 44px minmax(0,1fr)"));
        assert!(css.contains("line-height:22px"));
        assert!(css.contains("white-space:pre-wrap"));
        assert!(css.contains("overflow-wrap:anywhere"));
    }

    #[test]
    fn preview_css_uses_responsive_sidebar_columns() {
        let css = preview_css();
        assert!(css.contains("grid-template-columns:262px minmax(0,1fr) 252px"));
        assert!(css.contains("grid-template-columns:320px minmax(0,1fr) 304px"));
        assert!(css.contains("grid-template-columns:220px minmax(0,1fr) 210px"));
        assert!(css.contains("grid-template-columns:0 minmax(0,1fr) 0"));
        assert!(css.contains("@media (max-width:1280px)"));
        assert!(css.contains("@media (max-width:1024px)"));
        assert!(!css.contains("@media not all and (min-width:1280px)"));
        assert!(!css.contains("@media not all and (min-width:1024px)"));
        assert!(css.contains("grid-area:2/1"));
        assert!(css.contains("grid-area:2/2"));
        assert!(css.contains("grid-area:2/3"));
        assert!(!css.contains(".keybar{display:none}"));
    }

    #[test]
    fn preview_css_styles_the_route_recovery_grid() {
        let css = preview_css();

        assert_selector_declaration(
            css,
            ".grid-cols-\\[minmax\\(0\\,520px\\)\\]",
            "grid-template-columns:minmax(0,520px)",
        );
    }

    #[test]
    fn file_status_indicators_stay_compact_trailing_and_discreet() {
        // ! JS behavior: buildFileLeaf (li class, [name,status] child order, no icon) covered
        // ! by Vitest wheel.test.ts. Horizontal-wheel scroll math covered by computeWheelScroll
        // there.
        let css = preview_css();
        assert_selector_declaration(css, ".tstatus", "width:15px;height:15px");
        assert_selector_declaration(
            css,
            ".tfile.status-added>.tlabel",
            "color-mix(in srgb,var(--add-bg) 42%,transparent)",
        );
        assert_selector_declaration(
            css,
            ".tfile.status-deleted>.tlabel",
            "color-mix(in srgb,var(--del-bg) 42%,transparent)",
        );
        assert_selector_declaration(
            css,
            ".tstatus.status-added",
            "background-color:var(--add-bg)",
        );
    }

    #[test]
    fn commit_focus_highlight_is_wired() {
        // ! JS behavior: resolveActiveSet (toggle/member-set) covered by Vitest preview.test.ts.
        // ! owned-row DOM mutation is event-listener-only and not extracted.
        let css = preview_css();
        assert!(css.contains(".commit-focus .diff-unified .dl.owned"));
        assert!(css.contains(".commit-focus .diff-split .sp.owned"));
        assert!(css.contains("opacity:.34"));
        assert!(css.contains("inset 3px 0 0"));
    }
}
