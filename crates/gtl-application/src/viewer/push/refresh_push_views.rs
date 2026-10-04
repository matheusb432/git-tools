use gtl_models::paths::RepositoryRoot;

use crate::viewer::{
    ViewerState, ViewerStateError,
    work::{self, ReservedRecipeWork},
};

/// Updates the repository's live tabs; other tabs keep their reviewed snapshot.
#[cqrsy::command]
pub fn execute(
    path: &RepositoryRoot,
    viewer: &ViewerState,
) -> Result<Vec<ReservedRecipeWork>, ViewerStateError> {
    let tabs = viewer.inspect(|session| {
        session
            .tabs()
            .filter(|tab| tab.tab.live() && tab.recipe.cwd() == Some(path))
            .map(|tab| tab.tab.id())
            .collect::<Vec<_>>()
    })?;
    Ok(tabs
        .into_iter()
        .filter_map(|tab| work::reserve_refresh(viewer, tab).ok())
        .collect())
}
