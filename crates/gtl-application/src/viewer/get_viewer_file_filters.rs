use std::collections::BTreeSet;

use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::file_filters::ViewerFileFilters;

use super::{ViewerState, file_filters::FileFiltersError};

pub struct GetViewerFileFilters {
    pub tab_id: ViewerTabId,
}

#[cqrsy::query]
pub fn execute(
    request: &GetViewerFileFilters,
    state: &ViewerState,
) -> Result<ViewerFileFilters, FileFiltersError> {
    let snapshot = state
        .inspect(|session| session.content_snapshot(request.tab_id))?
        .ok_or(FileFiltersError::Changed)?;
    let view = snapshot.view();
    let mut paths = view
        .files
        .iter()
        .map(|file| &file.path)
        .collect::<BTreeSet<_>>();
    if let Some(applied) = &view.extension_filter {
        paths.extend(&applied.hidden_paths);
    }
    let mut extensions = BTreeSet::new();
    for path in paths {
        if let Some(extension) = path.as_path().extension().and_then(|value| value.to_str()) {
            extensions.insert(extension.to_ascii_lowercase());
        }
    }
    Ok(ViewerFileFilters {
        filter: view.file_filter.filter().clone(),
        extensions: extensions.into_iter().collect(),
    })
}
