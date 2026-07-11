//! Validated state rendered by the server-side viewer.

mod document;
mod ids;
mod options;
mod tabs;

pub use document::{
    ViewerDocument, ViewerDocumentError, ViewerHistoryEntry, ViewerSettings, ViewerView,
};
pub use ids::{RenderHistoryId, ViewerTabId};
pub use options::{DiffDensity, DiffLayout, ParseRenderOptionError, RenderOptions, Theme};
pub use tabs::{ViewerTab, ViewerTabKind, ViewerTabState};
