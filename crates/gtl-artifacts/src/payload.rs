use anyhow::{Context as _, Result, anyhow, ensure};
use gtl_application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, project_diff_lines, project_diff_view, project_render_options,
        project_theme,
    },
};
use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId};
use gtl_wire::viewer::{
    LoadViewerDiffLines, ViewerArtifactManifest, ViewerArtifactPage, ViewerArtifactPageId,
    ViewerCommitSelection, ViewerDiffCursor, ViewerViewIdentity,
};

pub(crate) struct ArtifactPayload {
    pub(crate) manifest: ViewerArtifactManifest,
    pub(crate) pages: Vec<ViewerArtifactPage>,
}

pub(crate) fn project_payload(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
) -> Result<ArtifactPayload> {
    let theme = theme.unwrap_or(Theme::Dark);
    let render_options = project_render_options(options);
    let mut active_views = Vec::with_capacity(views.len());
    let mut pages = Vec::new();

    for (index, view) in views.iter().enumerate() {
        let tab_id = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .context("artifact has too many views")?;
        let tab_id = ViewerTabId::try_new(tab_id).context("artifact view id must be positive")?;
        let identity = ViewerViewIdentity {
            tab_id,
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options,
        };
        let active = project_diff_view(view, view, identity, ViewerCommitSelection::None);
        project_pages(view, &active, &mut pages)?;
        active_views.push(active);
    }

    Ok(ArtifactPayload {
        manifest: ViewerArtifactManifest {
            title: title.to_owned(),
            theme: project_theme(theme),
            views: active_views,
        },
        pages,
    })
}

fn project_pages(
    view: &View,
    active: &gtl_wire::viewer::ViewerActiveView,
    pages: &mut Vec<ViewerArtifactPage>,
) -> Result<()> {
    for file in &active.files {
        let mut cursor = ViewerDiffCursor::default();
        loop {
            let request = LoadViewerDiffLines {
                identity: active.identity,
                file: file.id.clone(),
                cursor,
            };
            let page = project_diff_lines(view, &request).map_err(|error| {
                anyhow!(
                    "project artifact page for {} at {}: {error:?}",
                    file.id.as_str(),
                    cursor.into_inner()
                )
            })?;
            let next = page.next;
            let id = ViewerArtifactPageId::for_request(&request);
            pages.push(ViewerArtifactPage { id, page });
            let Some(next) = next else {
                break;
            };
            ensure!(
                next.into_inner() > cursor.into_inner(),
                "artifact page cursor did not advance"
            );
            cursor = next;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::{DiffDensity, DiffLayout};
    use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerTheme};

    use super::*;
    use crate::tests::sample_view;

    #[test]
    fn payload_projects_every_file_page_under_one_typed_identity() {
        let payload = project_payload(
            "artifact",
            &[sample_view()],
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            Some(Theme::Graphite),
        )
        .expect("project artifact payload");
        let active = &payload.manifest.views[0];

        assert_eq!(payload.manifest.theme, ViewerTheme::Graphite);
        assert_eq!(u64::from(active.identity.tab_id), 1);
        assert_eq!(
            active.identity.render_options.layout,
            ViewerDiffLayout::Split
        );
        assert_eq!(
            active.identity.render_options.density,
            ViewerDiffDensity::Full
        );
        assert_eq!(payload.pages.len(), active.files.len());
        assert!(payload.pages.iter().all(|page| {
            page.page.identity == active.identity
                && page.id.as_str()
                    == ViewerArtifactPageId::for_request(&LoadViewerDiffLines {
                        identity: page.page.identity,
                        file: page.page.file.clone(),
                        cursor: page.page.cursor,
                    })
                    .as_str()
        }));
    }
}
