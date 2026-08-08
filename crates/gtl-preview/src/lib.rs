//! Presentation for the diff preview: the shared `.layout` body, the offline
//! artifact documents, and the embedded stylesheet/JS assets. Consumed by the
//! daemon (artifact rendering through the `HtmlRenderer` port) and the desktop
//! viewer (`view_fragment` inside its htmx shell).

mod artifact;
mod assets;
mod comment_syntax;
mod layout;
mod rows;
mod syntax;
mod text;

pub use artifact::{build_html, build_tabbed_html};
pub use assets::{preview_bundle, preview_css, strip_stylesheet_banner};
use gtl_application::{
    diffs::View,
    viewer::{RenderOptions, ViewerTabId},
};
pub use layout::mobile_controls::{
    MobileViewControls, commits_navigation, files_navigation, mobile_menu_button_classes,
    mobile_menu_danger_button_classes, view_navigation,
};
use maud::Markup;
pub use syntax::{PreviewError, PreviewResult};

// TODO: [gtl-web]: replace the desktop fragment APIs with a narrow Maud diff-document renderer.
/// Builds one app-hosted diff view using the requested layout and density variant.
///
/// The fragment retains the server-rendered file tree, commit shelf, popovers, and diff rows,
/// while leaving layout, density, and theme controls to the surrounding app shell.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     diffs::View,
///     viewer::{RenderOptions, ViewerTabId},
/// };
/// use gtl_preview::view_fragment;
///
/// # fn load_view() -> View { todo!() }
/// # let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
/// let fragment = view_fragment(&load_view(), RenderOptions::DEFAULT, tab_id)
///     .expect("embedded syntax assets should load");
/// assert!(fragment.into_string().contains("diff-unified diff-compact"));
/// ```
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn view_fragment(
    view: &View,
    options: RenderOptions,
    tab_id: ViewerTabId,
) -> PreviewResult<Markup> {
    layout::view_body(view, options, layout::Surface::App { tab_id })
}

/// Builds an app-hosted diff view with server-rendered mobile controls.
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn view_fragment_with_mobile_controls(
    view: &View,
    range_view: &View,
    selected_commit_sha: Option<&str>,
    options: RenderOptions,
    tab_id: ViewerTabId,
    controls: MobileViewControls,
) -> PreviewResult<Markup> {
    layout::view_body_with_mobile_controls(
        view,
        range_view,
        selected_commit_sha,
        options,
        layout::Surface::App { tab_id },
        controls,
    )
}

// TODO: [gtl-web]: replace the HTMX shell API with a Dioxus-owned diff-island lifecycle.
/// Builds the desktop layout without diff rows and starts its bounded chunk chain.
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn view_shell(
    view: &View,
    options: RenderOptions,
    tab_id: ViewerTabId,
    load_id: u64,
) -> PreviewResult<Markup> {
    layout::view_body_shell(view, options, layout::Surface::App { tab_id }, load_id)
}

/// Builds the desktop layout shell with server-rendered mobile controls.
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn view_shell_with_mobile_controls(
    view: &View,
    range_view: &View,
    selected_commit_sha: Option<&str>,
    options: RenderOptions,
    tab_id: ViewerTabId,
    load_id: u64,
    controls: MobileViewControls,
) -> PreviewResult<Markup> {
    layout::view_body_shell_with_mobile_controls(
        view,
        range_view,
        selected_commit_sha,
        options,
        layout::Surface::App { tab_id },
        load_id,
        controls,
    )
}

/// One bounded server-rendered insertion into a file's existing diff container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewChunk {
    pub target_id: String,
    pub html: String,
    pub rows: usize,
}

/// Renders one chunk insertion and the continuation marker for its load chain.
pub fn view_chunk_fragment(chunk: &ViewChunk, next_load_id: Option<u64>) -> Markup {
    layout::view_chunk_fragment(chunk, next_load_id)
}

/// Renders the desktop diff rows into bounded, semantically ordered chunks.
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn view_chunks(
    view: &View,
    options: RenderOptions,
) -> PreviewResult<std::collections::VecDeque<ViewChunk>> {
    layout::view_chunks(view, options)
}

/// The Maud-backed [`HtmlRenderer`](gtl_application::ports::HtmlRenderer) adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaudRenderer;

impl gtl_application::ports::HtmlRenderer for MaudRenderer {
    fn build_html(
        &self,
        view: &View,
        options: RenderOptions,
        theme: Option<&str>,
    ) -> anyhow::Result<String> {
        build_html(view, options, theme).map_err(Into::into)
    }
    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<&str>,
    ) -> anyhow::Result<String> {
        build_tabbed_html(title, views, options, theme).map_err(Into::into)
    }
}

#[cfg(test)]
pub(crate) mod test_render {
    use gtl_application::{
        diffs::View,
        viewer::{RenderOptions, ViewerTabId},
    };
    use maud::Markup;

    use crate::syntax::SyntaxDefinition;

    pub(crate) fn build_html(view: &View, options: RenderOptions, theme: Option<&str>) -> String {
        crate::build_html(view, options, theme).expect("embedded syntax assets should load")
    }

    pub(crate) fn build_tabbed_html(
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<&str>,
    ) -> String {
        crate::build_tabbed_html(title, views, options, theme)
            .expect("embedded syntax assets should load")
    }

    pub(crate) fn view_fragment(
        view: &View,
        options: RenderOptions,
        tab_id: ViewerTabId,
    ) -> Markup {
        crate::view_fragment(view, options, tab_id).expect("embedded syntax assets should load")
    }

    pub(crate) fn syntax_for_path(path: &str) -> Option<SyntaxDefinition> {
        crate::syntax::syntax_for_path(path).expect("embedded syntax assets should load")
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use gtl_application::diffs::{Cmd, FileDiff, Foot, View};
    use gtl_models::diffs::{AppliedExclusions, Commit};

    /// Returns true if `html` contains any http(s):// URL. Enforces the
    /// offline-artifact contract: nothing in the artifact may trigger a
    /// network load.
    pub(crate) fn has_disallowed_external_url(html: &str) -> bool {
        html.match_indices("://")
            .any(|(sep, _)| html[..sep].ends_with("http") || html[..sep].ends_with("https"))
    }

    pub(crate) fn applied_exclusions() -> AppliedExclusions {
        AppliedExclusions {
            extensions: vec!["lock".to_string(), "md".to_string()],
            hidden_paths: vec!["docs/plan.md".to_string(), "Cargo.lock".to_string()],
        }
    }

    pub(crate) fn sample_view() -> View {
        View {
            exclusions: None,
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
        }
    }
}
