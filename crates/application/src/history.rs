//! The history feature: the desktop viewer's history-panel query.

pub mod list;
pub mod list_recent;
mod persistence;
pub mod record_render;

pub use persistence::RecentRenderRecord;
