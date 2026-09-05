//! Validated state rendered by the server-side viewer.

mod ids;
mod keybindings;
mod options;
mod pagination;
mod tabs;

pub use ids::{
    RenderHistoryId, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId, ViewerVersion,
};
pub use keybindings::{
    InvalidViewerKeybindings, ParseViewerKeybindingError, ViewerKeybinding, ViewerKeybindingAction,
    ViewerKeybindingDisplayKey, ViewerKeybindingPlatform, ViewerKeybindings,
    ViewerKeyboardModifier, ViewerKeyboardModifiers,
};
pub use options::{DiffDensity, DiffLayout, ParseRenderOptionError, RenderOptions, Theme};
pub use pagination::{
    HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
    InvalidHistoryPage,
};
pub use tabs::{ViewerTab, ViewerTabKind, ViewerTabPlacement, ViewerTabState};
