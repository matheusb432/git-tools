//! Presentation for complete offline diff artifacts and the server-rendered
//! diff document embedded by the desktop viewer.
//!
//! Offline artifacts retain their complete Maud-owned layout and inlined
//! enhancement assets. The desktop API renders only file shells and bounded
//! row chunks; its application shell belongs to the desktop frontend.

mod artifact;
mod assets;
mod comment_syntax;
mod layout;
mod rows;
mod syntax;
mod text;

pub use artifact::{build_html, build_tabbed_html};
use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::Markup;
pub use syntax::{PreviewError, PreviewResult};

/// Returns the stable DOM anchor used for a server-rendered diff file.
///
/// Hosts use this value to connect their own changed-file navigation to the
/// corresponding file block inside [`diff_document_shell`].
#[must_use]
pub fn diff_file_anchor_id(path: &str) -> String {
    text::slug(path)
}

/// Builds the server-rendered file document for an app-owned diff island.
///
/// File headers and actions render immediately. Diff row targets remain empty so the host can
/// append the bounded output from [`view_chunks`] without rendering rows in the client.
///
/// # Errors
///
/// Returns an error when the embedded rendering assets cannot be loaded.
pub fn diff_document_shell(view: &View, options: RenderOptions) -> PreviewResult<Markup> {
    layout::diff_document_shell(view, options)
}

/// One bounded server-rendered insertion into a file's existing diff container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewChunk {
    pub target_id: String,
    pub html: String,
    pub rows: usize,
}

/// Renders diff rows into bounded, semantically ordered chunks.
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
    use gtl_application::{diffs::View, viewer::RenderOptions};

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
