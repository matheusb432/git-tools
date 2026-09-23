//! Validated state rendered by the server-side viewer.

mod ids;
mod keybindings;
mod options;
mod pagination;
mod push_id;
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
pub use push_id::ViewerPushId;
pub use tabs::{ViewerTab, ViewerTabKind, ViewerTabPlacement, ViewerTabState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerSidebarVisibility {
    pub files: bool,
    pub commits: bool,
}

impl Default for ViewerSidebarVisibility {
    fn default() -> Self {
        Self {
            files: true,
            commits: true,
        }
    }
}
