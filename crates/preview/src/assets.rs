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
/// assert!(preview::preview_css().contains(".layout"));
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
        assert!(
            css.contains("@layer base{*,:before,:after{box-sizing:border-box;border:0 solid}}")
        );
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
    fn preview_css_styles_intra_line_word_spans() {
        let css = preview_css();
        assert!(css.contains(".diff-split .sp-del .ciw{"));
        assert!(css.contains(".diff-split .sp-add .ciw{"));
    }

    #[test]
    fn preview_css_drives_split_default_and_breakpoint_fallback() {
        let css = preview_css();
        // base: every pane hidden until a rule reveals exactly one
        assert!(css.contains(".filebody .diff{display:none}"));
        // unified is the default pane above the breakpoint (no data-diff-layout attr)
        assert!(css.contains("@media (min-width:1025px)"));
        assert!(css.contains(
            "html:not([data-diff-layout=split]):not([data-diff-full=on]) .diff-unified.diff-compact"
        ));
        // split is shown when the attr flips
        assert!(css.contains(
            "html[data-diff-layout=split]:not([data-diff-full=on]) .diff-split.diff-compact"
        ));
        // narrow screens force the combined pane and hide the layout toggle
        assert!(css.contains(".layout-toggle{display:none}"));
        // four-column split grid + the per-pane change colors
        assert!(css.contains(
            ".diff-split .dl{grid-template-columns:44px minmax(0,1fr) 44px minmax(0,1fr)"
        ));
        assert!(css.contains(".diff-split .sp-add{background:var(--add-bg);color:var(--add-ink)}"));
        assert!(css.contains(".diff-split .sp-del{background:var(--del-bg);color:var(--del-ink)}"));
    }

    #[test]
    fn preview_css_tames_long_lines_without_wrap() {
        let css = preview_css();
        assert!(
            css.contains(".dl-long .code-text,.diff-split code.long .code-text{white-space:pre")
        );
        assert!(css.contains(
            ".dl-long.expanded .code-text,.diff-split code.long.expanded .code-text{text-overflow:clip;overflow-x:auto}"
        ));
    }

    #[test]
    fn preview_css_wraps_diff_code_inside_fixed_line_number_gutters() {
        let css = preview_css();
        assert!(css.contains(".diff{font-size:14px;line-height:1.6;overflow-x:hidden}"));
        assert!(css.contains(
            ".dl{white-space:normal;grid-template-columns:44px 44px minmax(0,1fr);align-items:start;display:grid}"
        ));
        assert!(css.contains(".dl code{white-space:pre-wrap;overflow-wrap:anywhere;min-width:0"));
    }

    #[test]
    fn preview_css_uses_responsive_sidebar_columns() {
        let css = preview_css();
        assert!(css.contains("--tree-col:262px"));
        assert!(css.contains("--shelf-col:252px"));
        assert!(
            css.contains("grid-template-columns:var(--tree-col) minmax(0,1fr) var(--shelf-col)")
        );
        assert!(css.contains("@media (min-width:1600px) and (min-height:900px)"));
        assert!(css.contains("--tree-col:320px"));
        assert!(css.contains("--shelf-col:304px"));
        assert!(css.contains("@media (max-width:1280px)"));
        assert!(css.contains("--tree-col:220px"));
        assert!(css.contains("--shelf-col:210px"));
        assert!(css.contains("@media (max-width:1024px)"));
        assert!(css.contains("--side-display:none"));
        assert!(css.contains(".tdir>ul .tfile>.tlabel{padding-left:8px}"));
        // the 3-column shell: tree | main | shelf on the middle grid row
        assert!(css.contains("grid-area:2/1"));
        assert!(css.contains("grid-area:2/2"));
        assert!(css.contains("grid-area:2/3"));
        assert!(!css.contains(".keybar{display:none}"));
    }

    #[test]
    fn file_status_indicators_stay_compact_trailing_and_discreet() {
        // ! JS behavior: buildFileLeaf (li class, [name,status] child order, no icon) covered
        // ! by Vitest wheel.test.ts. Horizontal-wheel scroll math covered by computeWheelScroll
        // there.
        let css = preview_css();
        assert!(css.contains(".tfile.status-added>.tlabel"));
        assert!(css.contains(".tfile.status-deleted>.tlabel"));
        assert!(css.contains(".tlabel{"));
        assert!(css.contains("gap:5px;padding:2px 5px"));
        assert!(css.contains("gap:7px;padding:7px 10px;font-size:12.5px"));
    }

    #[test]
    fn commit_focus_highlight_is_wired() {
        // ! JS behavior: resolveActiveSet (toggle/member-set) covered by Vitest preview.test.ts.
        // ! owned-row DOM mutation is event-listener-only and not extracted.
        let css = preview_css();
        assert!(css.contains(".commit-focus .diff-unified .dl.owned{opacity:1}"));
        assert!(css.contains(".commit-focus .diff-split .sp.owned{opacity:1"));
        assert!(css.contains("inset 3px 0 0 var(--acc)"));
        assert!(css.contains("prefers-reduced-motion"));
    }
}
