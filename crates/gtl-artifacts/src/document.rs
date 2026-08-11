use std::fmt::Write as _;

use anyhow::{Context as _, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_application::{diffs::View, viewer::RenderOptions};
use gtl_contracts::viewer::VIEWER_ARTIFACT_MANIFEST_ID;

use crate::{assets, payload::project_payload};

/// Builds one self-contained, client-rendered diff artifact.
pub fn build_html(view: &View, options: RenderOptions, theme: Option<&str>) -> Result<String> {
    let count = view.commits.len();
    let suffix = if count == 1 { "" } else { "s" };
    let title = format!(
        "{} — {} · {count} commit{suffix}",
        view.repo_name, view.title
    );
    build_document(&title, std::slice::from_ref(view), options, theme)
}

/// Builds one self-contained artifact with a client-rendered tab per diff view.
pub fn build_tabbed_html(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<&str>,
) -> Result<String> {
    build_document(title, views, options, theme)
}

fn build_document(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<&str>,
) -> Result<String> {
    let payload = project_payload(title, views, options, theme)?;
    let runtime = assets::inline_runtime()?;
    ensure!(
        !assets::TAILWIND_CSS.contains("</style"),
        "generated Tailwind CSS cannot be embedded safely"
    );

    let mut html = String::with_capacity(
        assets::TAILWIND_CSS.len()
            + runtime.len()
            + payload
                .pages
                .iter()
                .map(|page| page.page.lines.iter().map(String::len).sum::<usize>())
                .sum::<usize>(),
    );
    write!(
        html,
        "<!doctype html><html lang=\"en\" data-theme=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark light\"><meta name=\"darkreader-lock\"><title>git-tools diff</title><style>{}</style></head><body><div id=\"main\"></div>",
        payload.manifest.theme.as_str(),
        assets::TAILWIND_CSS,
    )
    .context("write artifact document shell")?;
    write_payload_node(&mut html, VIEWER_ARTIFACT_MANIFEST_ID, &payload.manifest)?;
    for page in &payload.pages {
        write_payload_node(&mut html, page.id.as_str(), page)?;
    }
    write!(
        html,
        "<script type=\"module\">{runtime}</script></body></html>"
    )
    .context("write artifact runtime")?;
    Ok(html)
}

fn write_payload_node(
    html: &mut String,
    id: &str,
    payload: &(impl serde::Serialize + ?Sized),
) -> Result<()> {
    let bytes = serde_json::to_vec(payload).context("serialize artifact payload")?;
    write!(
        html,
        "<script id=\"{id}\" type=\"application/octet-stream\">{}</script>",
        STANDARD.encode(bytes)
    )
    .context("write artifact payload")
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{ViewerArtifactManifest, ViewerArtifactPage};

    use super::*;
    use crate::tests::{decode_payload, has_disallowed_external_url, sample_view};

    #[test]
    fn document_contains_only_inert_data_and_inline_client_assets() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, Some("dark"))
            .expect("build client-rendered artifact");
        let manifest: ViewerArtifactManifest = decode_payload(&html, VIEWER_ARTIFACT_MANIFEST_ID);
        let active = &manifest.views[0];
        let first_page_id = gtl_contracts::viewer::ViewerArtifactPageId::for_request(
            &gtl_contracts::viewer::LoadViewerDiffLines {
                identity: active.identity,
                file: active.files[0].id.clone(),
                cursor: gtl_contracts::viewer::ViewerDiffCursor::START,
            },
        );
        let page: ViewerArtifactPage = decode_payload(&html, first_page_id.as_str());

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("<script type=\"module\">"));
        assert!(!html.contains("+client-rendered"));
        assert!(!has_disallowed_external_url(&html));
        assert_eq!(manifest.title, "api — diff · 0 commits");
        assert_eq!(page.page.lines[1], "+client-rendered");
    }

    #[test]
    fn tabbed_document_assigns_distinct_view_and_page_addresses() {
        let view = sample_view();
        let html = build_tabbed_html(
            "subrepo diff",
            &[view.clone(), view],
            RenderOptions::DEFAULT,
            None,
        )
        .expect("build tabbed client-rendered artifact");
        let manifest: ViewerArtifactManifest = decode_payload(&html, VIEWER_ARTIFACT_MANIFEST_ID);

        assert_eq!(manifest.views.len(), 2);
        assert_ne!(
            manifest.views[0].identity.tab_id,
            manifest.views[1].identity.tab_id
        );
        for active in &manifest.views {
            let id = gtl_contracts::viewer::ViewerArtifactPageId::for_request(
                &gtl_contracts::viewer::LoadViewerDiffLines {
                    identity: active.identity,
                    file: active.files[0].id.clone(),
                    cursor: gtl_contracts::viewer::ViewerDiffCursor::START,
                },
            );
            let page: ViewerArtifactPage = decode_payload(&html, id.as_str());
            assert_eq!(page.page.identity, active.identity);
        }
    }
}
