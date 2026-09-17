//! Native build-time rendering for self-contained static diff artifacts.

mod assets;
mod document;
mod payload;

pub use document::{build_html, build_tabbed_html};
use gtl_application::{
    diffs::View,
    viewer::{RenderOptions, Theme},
};

/// The [`HtmlRenderer`](gtl_application::ports::HtmlRenderer) adapter for offline artifacts.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArtifactRenderer;

impl gtl_application::ports::HtmlRenderer for ArtifactRenderer {
    fn build_html(
        &self,
        view: &View,
        options: RenderOptions,
        theme: Option<Theme>,
    ) -> anyhow::Result<String> {
        build_html(view, options, theme)
    }

    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<Theme>,
    ) -> anyhow::Result<String> {
        build_tabbed_html(title, views, options, theme)
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::diffs::{Cmd, FileDiff, Foot, View};
    use gtl_models::{
        diffs::DiffLineCount,
        git::{BranchName, GitHead, GitRevision},
    };

    pub(crate) fn sample_view() -> View {
        View {
            file_filter: gtl_application::diffs::file_filter::DiffFileFilter::default(),
            exclusions: None,
            repo_name: gtl_models::paths::ProjectName::try_from("api").unwrap(),
            repo_root: gtl_models::paths::RepositoryRoot::try_new("/repo/api".into()).unwrap(),
            branch: GitHead::Branch(BranchName::try_new("main").unwrap()),
            upstream: GitRevision::try_new("origin/main").unwrap(),
            commits: Vec::new(),
            files: vec![FileDiff {
                path: gtl_models::paths::RepositoryRelativePath::try_new("src/lib.rs".into())
                    .unwrap(),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::new(1),
                lines: vec!["@@ -1 +1 @@".to_owned(), "+static_rendered".to_owned()].into(),
                full_lines: Some(
                    vec![
                        "@@ -1 +1 @@".to_owned(),
                        "+static_rendered".to_owned(),
                        " context".to_owned(),
                    ]
                    .into(),
                ),
            }],
            title: "diff".to_owned(),
            cmd: Cmd {
                lead: "git diff ".to_owned(),
                range: "origin/main..HEAD".to_owned(),
                trail: String::new(),
            },
            commits_label: "0 commits".to_owned(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".to_owned(),
            },
            full_context: gtl_application::diffs::FullContextDiffState::Loaded,
        }
    }

    pub(crate) fn has_disallowed_external_url(html: &str) -> bool {
        [" src=", " href=", " srcset=", " action=", "url("]
            .iter()
            .any(|reference| html.contains(reference))
    }
}
