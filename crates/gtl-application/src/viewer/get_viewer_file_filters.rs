use std::collections::BTreeSet;

use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::file_filters::ViewerFileFilters;
use rusqlite::Connection;

use super::{ViewerState, file_filters::FileFiltersError};
use crate::{
    ports::UserSettingsReader,
    projects::find_project_by_repository::{self, FindProjectByRepository},
};

pub struct GetViewerFileFilters {
    pub tab_id: ViewerTabId,
}

#[cqrsy::query]
pub fn execute(
    request: &GetViewerFileFilters,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    connection: &Connection,
) -> Result<ViewerFileFilters, FileFiltersError> {
    let snapshot = state
        .inspect(|session| session.content_snapshot(request.tab_id))?
        .ok_or(FileFiltersError::Changed)?;
    let view = snapshot.view();
    let project = find_project_by_repository::execute(
        &FindProjectByRepository {
            path: &view.repo_root,
        },
        connection,
    )?
    .map(|_| view.repo_name.clone());
    let configured = settings.load()?;
    let saved = project
        .as_ref()
        .and_then(|project| configured.diff_exclusions().for_project(project).cloned());
    let mut paths = view
        .files
        .iter()
        .map(|file| &file.path)
        .collect::<BTreeSet<_>>();
    if let Some(exclusions) = &view.exclusions {
        paths.extend(&exclusions.hidden_paths);
    }
    let mut extensions = BTreeSet::new();
    for path in paths {
        if let Some(extension) = path.as_path().extension().and_then(|value| value.to_str()) {
            extensions.insert(extension.to_lowercase());
        }
    }
    Ok(ViewerFileFilters {
        excluded: view.file_filter.excluded().clone(),
        extensions: extensions.into_iter().collect(),
        project,
        saved,
        defaults: configured.diff_exclusions().default_exclusions().clone(),
    })
}
