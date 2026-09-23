use gtl_models::paths::RepositoryRoot;

use crate::viewer::{
    ViewerState, ViewerStateError,
    work::{self, ReservedRecipeWork},
};

/// Recomputes affected live views; immutable snapshots retain their reviewed content.
#[cqrsy::command]
pub fn execute(
    path: &RepositoryRoot,
    viewer: &ViewerState,
) -> Result<Vec<ReservedRecipeWork>, ViewerStateError> {
    let tabs = viewer.inspect(|session| {
        session
            .tabs()
            .filter(|tab| {
                !matches!(tab.tab.kind(), gtl_models::viewer::ViewerTabKind::Snapshot)
                    && tab.recipe.cwd() == *path
            })
            .map(|tab| tab.tab.id())
            .collect::<Vec<_>>()
    })?;
    Ok(tabs
        .into_iter()
        .filter_map(|tab| work::reserve_refresh(viewer, tab).ok())
        .collect())
}
