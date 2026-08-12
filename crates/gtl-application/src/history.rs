//! The history feature: the desktop viewer's history-panel query.

pub mod get_recent_render;
pub mod list;
pub mod list_recent_render_page;
mod persistence;
pub mod record_render;

pub use persistence::{RecentRenderRecord, RecentRenderRowError};
