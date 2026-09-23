use std::sync::Arc;

use gtl_models::diffs::ExcludedExtensions;
use gtl_wire::viewer::{
    FieldUpdate,
    file_filters::{SetViewerFileFilters, UpdateDiffExclusions},
};
use rusqlite::Connection;

use super::{
    ViewerDiffSnapshot, ViewerState, file_filters::FileFiltersError,
    session::file_filters::FileFilterViews, update_diff_exclusions,
};
use crate::{
    diffs::set_diff_file_exclusions::{self, SetDiffFileExclusions},
    ports::{GitClient, UserSettingsEditor, UserSettingsReader},
    projects::find_project_by_repository::{self, FindProjectByRepository},
};

pub struct PreparedFileFilterChange {
    views: FileFilterViews,
    excluded: ExcludedExtensions,
    persistence: Option<UpdateDiffExclusions>,
}

pub fn prepare(
    request: SetViewerFileFilters,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    connection: &Connection,
) -> Result<PreparedFileFilterChange, FileFiltersError> {
    let views = state
        .inspect(|session| session.file_filter_views(request.tab_id))?
        .ok_or(FileFiltersError::Changed)?;
    let project = find_project_by_repository::execute(
        &FindProjectByRepository {
            path: &views.expected.repo_root,
        },
        connection,
    )?
    .map(|_| views.expected.repo_name.clone());
    let excluded = match &request.exclusions {
        FieldUpdate::Update(value) => value.clone(),
        FieldUpdate::Clear => settings
            .load()?
            .diff_exclusions()
            .default_exclusions()
            .clone(),
        FieldUpdate::Unchanged => views.expected.file_filter.excluded().clone(),
    };
    let persistence = project.map(|project| UpdateDiffExclusions {
        project: Some(project),
        extensions: request.exclusions,
        expected: request.expected,
    });
    Ok(PreparedFileFilterChange {
        views,
        excluded,
        persistence,
    })
}

#[cqrsy::command]
pub fn execute(
    request: PreparedFileFilterChange,
    state: &ViewerState,
    settings: &mut impl UserSettingsEditor,
    git: &impl GitClient,
) -> Result<(), FileFiltersError> {
    let PreparedFileFilterChange {
        mut views,
        excluded,
        persistence,
    } = request;
    let changed = views.expected.file_filter.excluded() != &excluded;
    if changed {
        let update = |view: ViewerDiffSnapshot| -> Result<ViewerDiffSnapshot, FileFiltersError> {
            let view = set_diff_file_exclusions::execute(
                SetDiffFileExclusions {
                    view: view.shared_view(),
                    excluded: excluded.clone(),
                },
                git,
            )?;
            Ok(state.prepare_snapshot(Arc::new(view))?)
        };
        views.range = update(views.range)?;
        views.selected = views.selected.map(update).transpose()?;
        views.modified = views.modified.map(update).transpose()?;
    }
    if changed {
        state.update(|session| session.publish_file_filters(views, excluded))??;
    }
    if let Some(persistence) = persistence {
        update_diff_exclusions::execute(persistence, settings, state)?;
    }
    Ok(())
}
