//! Viewer application operations and their public document value types.

mod complete_recipe_computation;
mod compute_recipe;
mod diff_view;
mod dto;
pub mod initial_recipe_label;
mod logic;
pub mod prepare_recipe;
mod probe_recipe;

pub use diff_view::{diff_file_anchor_id, project_diff_lines, project_diff_view};
pub use dto::{
    ViewerDocument, ViewerDocumentError, ViewerHistoryEntry, ViewerHistoryPage, ViewerSettings,
    ViewerView,
};
pub use gtl_models::viewer::{
    DiffDensity, DiffLayout, ParseRenderOptionError, RenderHistoryId, RenderOptions, Theme,
    ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState,
};
