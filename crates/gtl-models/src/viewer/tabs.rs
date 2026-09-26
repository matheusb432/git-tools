use serde::{Deserialize, Serialize};

use super::ViewerTabId;
use crate::{
    failure::{Failure, ViewerFailure},
    recipes::RecipeLabel,
};

/// Describes whether a viewer tab can currently provide rendered diff content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerTabState {
    /// Indicates that the tab's view is still being computed.
    Pending,
    /// Indicates that the tab has a corresponding rendered view.
    Ready,
    /// Indicates that the tab's source is broken; a live tab updates again when it recovers.
    Broken { failure: ViewerFailure },
    /// Indicates that computing or rendering the view failed.
    Error { failure: Failure },
}

/// Places a moved viewer tab relative to another tab with stable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerTabPlacement {
    /// Places the moved tab immediately before the target tab.
    Before,
    /// Places the moved tab immediately after the target tab.
    After,
}

/// Holds one tab-strip entry whose identity and rendering state are authoritative.
///
/// Every tab shows a snapshot of its recipe; a live tab updates that snapshot when its source
/// changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerTab {
    id: ViewerTabId,
    label: RecipeLabel,
    live: bool,
    state: ViewerTabState,
}

impl ViewerTab {
    /// Creates a tab-strip entry from validated identity and closed state values.
    #[must_use]
    pub fn new(id: ViewerTabId, label: RecipeLabel, live: bool, state: ViewerTabState) -> Self {
        Self {
            id,
            label,
            live,
            state,
        }
    }

    /// Returns the validated tab identity.
    #[must_use]
    pub const fn id(&self) -> ViewerTabId {
        self.id
    }

    /// Returns the tab-strip label.
    #[must_use]
    pub const fn label(&self) -> &RecipeLabel {
        &self.label
    }

    /// Returns whether the tab updates when its source changes.
    #[must_use]
    pub const fn live(&self) -> bool {
        self.live
    }

    /// Returns the tab's authoritative rendering state.
    #[must_use]
    pub const fn state(&self) -> &ViewerTabState {
        &self.state
    }
}
