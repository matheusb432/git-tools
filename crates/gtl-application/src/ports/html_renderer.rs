use gtl_models::{
    settings::ViewerLanguage,
    viewer::{RenderOptions, Theme},
};

use crate::diffs::View;

/// Renders a diff [`View`] to a self-contained HTML document.
pub trait HtmlRenderer: Clone + Send + Sync + 'static {
    /// The complete `file://`-ready HTML for `view`.
    fn build_html(
        &self,
        view: &View,
        options: RenderOptions,
        theme: Option<Theme>,
        language: ViewerLanguage,
    ) -> anyhow::Result<String>;

    /// Renders several views in one document with a linked section per repository.
    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<Theme>,
        language: ViewerLanguage,
    ) -> anyhow::Result<String>;
}
