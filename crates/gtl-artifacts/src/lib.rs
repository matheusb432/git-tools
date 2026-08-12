//! Deterministic packing for self-contained, client-rendered diff artifacts.

mod assets;
mod compression;
mod document;
mod payload;

pub use document::{build_html, build_tabbed_html};
use gtl_application::{diffs::View, viewer::RenderOptions};

/// The [`HtmlRenderer`](gtl_application::ports::HtmlRenderer) adapter for offline artifacts.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArtifactRenderer;

impl gtl_application::ports::HtmlRenderer for ArtifactRenderer {
    fn build_html(
        &self,
        view: &View,
        options: RenderOptions,
        theme: Option<&str>,
    ) -> anyhow::Result<String> {
        build_html(view, options, theme)
    }

    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[View],
        options: RenderOptions,
        theme: Option<&str>,
    ) -> anyhow::Result<String> {
        build_tabbed_html(title, views, options, theme)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use gtl_application::diffs::{Cmd, FileDiff, Foot, View};
    use serde::de::DeserializeOwned;

    pub(crate) fn sample_view() -> View {
        View {
            exclusions: None,
            repo_name: "api".to_owned(),
            repo_root: "/repo/api".to_owned(),
            branch: "main".to_owned(),
            upstream: "origin/main".to_owned(),
            commits: Vec::new(),
            files: vec![FileDiff {
                path: "src/lib.rs".to_owned(),
                added: 1,
                removed: 1,
                lines: vec!["@@ -1 +1 @@".to_owned(), "+client-rendered".to_owned()],
                full_lines: Some(vec![
                    "@@ -1 +1 @@".to_owned(),
                    "+client-rendered".to_owned(),
                    " context".to_owned(),
                ]),
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
        }
    }

    pub(crate) fn decode_payload<T: DeserializeOwned>(html: &str, id: &str) -> T {
        let prefix = format!("<script id=\"{id}\" type=\"application/octet-stream\"");
        let encoded = html
            .split_once(&prefix)
            .and_then(|(_, tail)| tail.split_once('>'))
            .and_then(|(_, tail)| tail.split_once("</script>"))
            .map(|(encoded, _)| encoded)
            .expect("artifact payload node");
        let compressed = STANDARD.decode(encoded).expect("base64 artifact payload");
        let bytes = crate::compression::gunzip(&compressed).expect("gzip artifact payload");
        serde_json::from_slice(&bytes).expect("JSON artifact payload")
    }

    pub(crate) fn has_disallowed_external_url(html: &str) -> bool {
        let without_tailwind_attribution = html.replace("https://tailwindcss.com", "");
        without_tailwind_attribution
            .match_indices("://")
            .any(|(separator, _)| {
                without_tailwind_attribution[..separator].ends_with("http")
                    || without_tailwind_attribution[..separator].ends_with("https")
            })
    }
}
