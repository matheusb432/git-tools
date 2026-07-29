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

use application::{
    diffs::View,
    viewer::{RenderOptions, ViewerTabId},
};
pub use artifact::{build_html, build_tabbed_html};
pub use assets::{preview_bundle, preview_css, strip_stylesheet_banner};
use maud::Markup;

/// Builds one app-hosted diff view using the requested layout and density variant.
///
/// The fragment retains the server-rendered file tree, commit shelf, popovers, and diff rows,
/// while leaving layout, density, and theme controls to the surrounding app shell.
///
/// # Examples
///
/// ```no_run
/// use application::{
///     diffs::View,
///     viewer::{RenderOptions, ViewerTabId},
/// };
/// use preview::view_fragment;
///
/// # fn load_view() -> View { todo!() }
/// # let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
/// let fragment = view_fragment(&load_view(), RenderOptions::DEFAULT, tab_id);
/// assert!(fragment.into_string().contains("diff-unified diff-compact"));
/// ```
pub fn view_fragment(view: &View, options: RenderOptions, tab_id: ViewerTabId) -> Markup {
    layout::view_body(view, options, layout::Surface::App { tab_id })
}

/// Builds the desktop layout without diff rows and starts its bounded chunk chain.
pub fn view_shell(
    view: &View,
    options: RenderOptions,
    tab_id: ViewerTabId,
    load_id: u64,
) -> Markup {
    layout::view_body_shell(view, options, layout::Surface::App { tab_id }, load_id)
}

/// One bounded server-rendered insertion into a file's existing diff container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewChunk {
    pub target_id: String,
    pub html: String,
    pub rows: usize,
}

/// Renders the desktop diff rows into bounded, semantically ordered chunks.
pub fn view_chunks(view: &View, options: RenderOptions) -> std::collections::VecDeque<ViewChunk> {
    layout::view_chunks(view, options)
}

/// The Maud-backed [`HtmlRenderer`](application::ports::HtmlRenderer) adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaudRenderer;

impl application::ports::HtmlRenderer for MaudRenderer {
    fn build_html(&self, view: &View, options: RenderOptions, theme: Option<&str>) -> String {
        build_html(view, options, theme)
    }
    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<&str>,
    ) -> String {
        build_tabbed_html(title, views, options, theme)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use application::diffs::{Cmd, FileDiff, Foot, LineOwners, View};
    use domain::diffs::{AppliedExclusions, Commit};

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
        }
    }
}
