use std::sync::Arc;

use gtl_models::diffs::ExtensionFilter;
use gtl_wire::viewer::file_filters::SetViewerFileFilters;

use super::{
    ViewerDiffSnapshot, ViewerState, file_filters::FileFiltersError,
    session::file_filters::FileFilterViews,
};
use crate::{
    diffs::set_diff_extension_filter::{self, SetDiffExtensionFilter},
    ports::{ExtensionFilterWriter, GitClient},
};

pub struct PreparedFileFilterChange {
    views: FileFilterViews,
    filter: ExtensionFilter,
}

pub fn prepare(
    request: SetViewerFileFilters,
    state: &ViewerState,
) -> Result<PreparedFileFilterChange, FileFiltersError> {
    let views = state
        .inspect(|session| session.file_filter_views(request.tab_id))?
        .ok_or(FileFiltersError::Changed)?;
    Ok(PreparedFileFilterChange {
        views,
        filter: request.filter,
    })
}

/// Applies the filter to the tab's snapshots, then saves it for the tab's repository.
#[cqrsy::command]
pub fn execute(
    request: PreparedFileFilterChange,
    state: &ViewerState,
    git: &impl GitClient,
    filters: &impl ExtensionFilterWriter,
) -> Result<(), FileFiltersError> {
    let PreparedFileFilterChange { mut views, filter } = request;
    let repository = views.expected.repo_root.clone();
    if views.expected.file_filter.filter() != &filter {
        let update = |view: ViewerDiffSnapshot| -> Result<ViewerDiffSnapshot, FileFiltersError> {
            let view = set_diff_extension_filter::execute(
                SetDiffExtensionFilter {
                    view: view.shared_view(),
                    filter: filter.clone(),
                },
                git,
            )?;
            Ok(state.prepare_snapshot(Arc::new(view))?)
        };
        views.range = update(views.range)?;
        views.selected = views.selected.map(update).transpose()?;
        views.modified = views.modified.map(update).transpose()?;
        state.update(|session| session.publish_file_filters(views, filter.clone()))??;
    }
    filters.save_extension_filter(&repository, &filter)?;
    Ok(())
}
