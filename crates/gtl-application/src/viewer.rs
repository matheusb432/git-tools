//! Viewer application operations and shared model re-exports.

pub mod close_viewer_tabs;
mod complete_recipe_computation;
mod compute_recipe;
mod diff_view;
pub mod ensure_view_full_context;
pub mod find_viewer_diff;
pub mod get_viewer_shell;
pub mod initial_recipe_label;
pub mod move_viewer_tab;
pub mod open_viewer_diff_file;
pub mod prepare_recipe;
mod probe_recipe;
pub mod read_viewer_diff_text;
mod recipe_label;
pub mod refresh_live_view;
pub mod rows;
pub mod search_viewer_files;
pub mod session;
pub mod settings;
pub mod shell;
pub mod source;
mod state;
pub mod work;

pub use diff_view::{
    ViewerDiffFileSource, ViewerDiffSnapshot, diff_file_anchor_id, project_diff_view,
    project_render_options, project_theme, viewer_diff_file_source,
};
pub use gtl_models::viewer::{
    DiffDensity, DiffLayout, ParseRenderOptionError, RenderHistoryId, RenderOptions, Theme,
    ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState,
};
pub use state::{ViewerState, ViewerStateError};

pub mod set_modified_files;

pub mod pinned_tabs;
