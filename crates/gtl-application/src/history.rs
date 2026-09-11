//! The history feature: the desktop viewer's history-panel query.

pub mod associate_render_projects;
pub mod copy_render;
pub mod get_recent_render;
pub mod list_history;
pub mod list_recent_render_page;
mod persistence;
pub mod record_render;

pub use persistence::{RecentRenderRecord, RecentRenderRowError};
