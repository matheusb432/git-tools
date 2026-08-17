//! Validated state rendered by the server-side viewer.

mod ids;
mod options;
mod pagination;
mod tabs;

pub use ids::{
    RenderHistoryId, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerShellRevision,
    ViewerTabId,
};
pub use options::{DiffDensity, DiffLayout, ParseRenderOptionError, RenderOptions, Theme};
pub use pagination::{
    HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
    InvalidHistoryPage,
};
pub use tabs::{ViewerTab, ViewerTabKind, ViewerTabState};
