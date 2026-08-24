//! Viewer application operations and shared model re-exports.

mod complete_recipe_computation;
mod compute_recipe;
mod diff_view;
pub mod initial_recipe_label;
pub mod prepare_recipe;
mod probe_recipe;
mod recipe_label;
pub mod rows;
pub mod session;
pub mod shell;
mod state;
pub mod work;

pub use diff_view::{
    ViewerDiffFileSource, diff_file_anchor_id, project_diff_view, project_render_options,
    project_theme, viewer_diff_file_source,
};
pub use gtl_models::viewer::{
    DiffDensity, DiffLayout, ParseRenderOptionError, RenderHistoryId, RenderOptions, Theme,
    ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState,
};
pub use state::{ViewerState, ViewerStateError};
