use anyhow::{Context as _, Result};
use gtl_application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, project_diff_view, project_render_options, project_theme,
        rows::parse_viewer_diff_file, viewer_diff_file_source,
    },
};
use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId};
use gtl_web::StaticArtifactFileRows;
use gtl_wire::viewer::{ViewerActiveView, ViewerCommitSelection, ViewerTheme, ViewerViewIdentity};

pub(crate) struct ArtifactPayload {
    pub(crate) theme: ViewerTheme,
    pub(crate) views: Vec<ProjectedArtifactView>,
}

pub(crate) struct ProjectedArtifactView {
    pub(crate) view: ViewerActiveView,
    pub(crate) rows: Vec<StaticArtifactFileRows>,
}

pub(crate) fn project_payload(
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
) -> Result<ArtifactPayload> {
    let theme = theme.unwrap_or_default();
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
        let rows = project_rows(view, &active)?;
        projected_views.push(ProjectedArtifactView { view: active, rows });
    }

    Ok(ArtifactPayload {
        theme: project_theme(theme),
        views: projected_views,
    })
}

fn project_rows(view: &View, active: &ViewerActiveView) -> Result<Vec<StaticArtifactFileRows>> {
    active
        .files
        .iter()
        .map(|file| {
            let source =
                viewer_diff_file_source(view, &file.id, active.identity.render_options.density)
                    .with_context(|| format!("project artifact source for {}", file.id.as_str()))?;
            let parsed = parse_viewer_diff_file(
                source.path,
                source.lines,
                active.identity.render_options.layout,
            );
            Ok(StaticArtifactFileRows {
                file: file.id.clone(),
                rows: parsed.file,
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
        .unwrap();
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
        assert_eq!(projected.rows.len(), projected.view.files.len());
        assert_eq!(projected.rows[0].file, projected.view.files[0].id);
        assert!(matches!(
            &projected.rows[0].rows.rows,
            gtl_wire::viewer::ViewerRows::Split(rows) if !rows.is_empty()
        ));
    }

    #[test]
    fn payload_parses_every_source_line_before_static_rendering() {
        let mut view = sample_view();
        let lines = (0..300)
            .map(|index| format!("+{index:03}-{}", "x".repeat(1_000)))
            .collect::<Vec<_>>();
        view.files[0].lines = lines.clone().into();
        view.files[0].full_lines = Some(view.files[0].lines.clone());

        let payload = project_payload(&[view], RenderOptions::DEFAULT, None).unwrap();

        assert!(matches!(
            &payload.views[0].rows[0].rows.rows,
            gtl_wire::viewer::ViewerRows::Unified(rows) if rows.len() == lines.len()
        ));
    }
}
