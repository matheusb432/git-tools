use serde::{Deserialize, Serialize};

use super::ViewerTabId;

/// Describes whether a viewer tab can currently provide rendered diff content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerTabState {
    /// Indicates that the tab has a corresponding rendered view.
    Ready,
    /// Indicates that a known protocol or persistence failure prevents rendering.
    Broken { code: String, reason: String },
    /// Indicates that an unexpected runtime failure prevents rendering.
    Error { reason: String },
}

/// Distinguishes immutable snapshots from regenerating live tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerTabKind {
    /// Represents an immutable stored render.
    Snapshot,
    /// Represents a view regenerated from a source recipe.
    Live,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerTab {
    id: ViewerTabId,
    label: String,
    kind: ViewerTabKind,
    state: ViewerTabState,
}

impl ViewerTab {
    /// Creates a tab-strip entry from validated identity and closed state values.
    #[must_use]
    pub fn new(id: ViewerTabId, label: String, kind: ViewerTabKind, state: ViewerTabState) -> Self {
        Self {
            id,
            label,
            kind,
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
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the tab is a snapshot or live view.
    #[must_use]
    pub const fn kind(&self) -> ViewerTabKind {
        self.kind
    }

    /// Returns the tab's authoritative rendering state.
    #[must_use]
    pub const fn state(&self) -> &ViewerTabState {
        &self.state
    }
}
