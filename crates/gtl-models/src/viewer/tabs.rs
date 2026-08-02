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
    pub fn new(id: ViewerTabId, label: String, kind: ViewerTabKind, state: ViewerTabState) -> Self {
        Self {
            id,
            label,
            kind,
            state,
        }
    }

    /// Returns the validated tab identity.
    pub const fn id(&self) -> ViewerTabId {
        self.id
    }

    /// Returns the tab-strip label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the tab is a snapshot or live view.
    pub const fn kind(&self) -> ViewerTabKind {
        self.kind
    }

    /// Returns the tab's authoritative rendering state.
    pub const fn state(&self) -> &ViewerTabState {
        &self.state
    }
}
