use anyhow::{Context as _, Result, anyhow, ensure};
use gtl_application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, project_diff_lines, project_diff_view, project_render_options,
        project_theme,
    },
};
use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId};
use gtl_web::StaticArtifactFileSource;
use gtl_wire::viewer::{
    LoadViewerDiffLines, ViewerActiveView, ViewerCommitSelection, ViewerDiffCursor, ViewerTheme,
    ViewerViewIdentity,
};

pub(crate) struct ArtifactPayload {
    pub(crate) theme: ViewerTheme,
    pub(crate) views: Vec<ProjectedArtifactView>,
}

pub(crate) struct ProjectedArtifactView {
    pub(crate) view: ViewerActiveView,
    pub(crate) sources: Vec<StaticArtifactFileSource>,
}

pub(crate) fn project_payload(
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
) -> Result<ArtifactPayload> {
    let theme = theme.unwrap_or(Theme::Dark);
    let render_options = project_render_options(options);
    let mut projected_views = Vec::with_capacity(views.len());

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
        let sources = project_sources(view, &active)?;
        projected_views.push(ProjectedArtifactView {
            view: active,
            sources,
        });
    }

    Ok(ArtifactPayload {
        theme: project_theme(theme),
        views: projected_views,
    })
}

fn project_sources(
    view: &View,
    active: &ViewerActiveView,
) -> Result<Vec<StaticArtifactFileSource>> {
    active
        .files
        .iter()
        .map(|file| {
            let mut cursor = ViewerDiffCursor::default();
            let mut lines = Vec::new();
            loop {
                let request = LoadViewerDiffLines {
                    identity: active.identity,
                    file: file.id.clone(),
                    cursor,
                };
                let page = project_diff_lines(view, &request).map_err(|error| {
                    anyhow!(
                        "project artifact source for {} at {}: {error:?}",
                        file.id.as_str(),
                        cursor.into_inner()
                    )
                })?;
                ensure!(
                    page.identity == request.identity
                        && page.file == request.file
                        && page.cursor == request.cursor,
                    "projected artifact source changed identity"
                );
                lines.extend(page.lines);
                let Some(next) = page.next else {
                    break;
                };
                ensure!(
                    next.into_inner() > cursor.into_inner(),
                    "artifact source cursor did not advance"
                );
                cursor = next;
            }
            Ok(StaticArtifactFileSource {
                file: file.id.clone(),
                lines,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::{DiffDensity, DiffLayout};
    use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerTheme};

    use super::*;
    use crate::tests::sample_view;

    #[test]
    fn payload_projects_every_file_source_under_one_typed_identity() {
        let payload = project_payload(
            &[sample_view()],
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            Some(Theme::Graphite),
        )
        .expect("project artifact payload");
        let projected = &payload.views[0];

        assert_eq!(payload.theme, ViewerTheme::Graphite);
        assert_eq!(u64::from(projected.view.identity.tab_id), 1);
        assert_eq!(
            projected.view.identity.render_options.layout,
            ViewerDiffLayout::Split
        );
        assert_eq!(
            projected.view.identity.render_options.density,
            ViewerDiffDensity::Full
        );
        assert_eq!(projected.sources.len(), projected.view.files.len());
        assert_eq!(projected.sources[0].file, projected.view.files[0].id);
        assert_eq!(
            projected.sources[0].lines,
            ["@@ -1 +1 @@", "+static_rendered", " context"]
        );
    }

    #[test]
    fn payload_gathers_every_bounded_page_before_static_rendering() {
        let mut view = sample_view();
        let lines = (0..300)
            .map(|index| format!("+{index:03}-{}", "x".repeat(1_000)))
            .collect::<Vec<_>>();
        view.files[0].lines.clone_from(&lines);
        view.files[0].full_lines = Some(lines.clone());

        let payload = project_payload(&[view], RenderOptions::DEFAULT, None)
            .expect("project multi-page artifact source");

        assert_eq!(payload.views[0].sources[0].lines, lines);
    }
}
