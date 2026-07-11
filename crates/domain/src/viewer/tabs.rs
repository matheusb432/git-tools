use super::ViewerTabId;

/// Describes whether a viewer tab can currently provide rendered diff content.
///
/// # Examples
///
/// ```
/// use domain::viewer::ViewerTabState;
///
/// assert_eq!(ViewerTabState::Ready, ViewerTabState::Ready);
/// ```
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
///
/// # Examples
///
/// ```
/// use domain::viewer::ViewerTabKind;
///
/// assert_ne!(ViewerTabKind::Snapshot, ViewerTabKind::Live);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerTabKind {
    /// Represents an immutable stored render.
    Snapshot,
    /// Represents a view regenerated from a source recipe.
    Live,
}

/// Holds one tab-strip entry whose identity and rendering state are authoritative.
///
/// # Examples
///
/// ```
/// use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
///
/// let id = ViewerTabId::try_new(1).expect("positive id");
/// let tab = ViewerTab::new(
///     id,
///     "Changes".into(),
///     ViewerTabKind::Snapshot,
///     ViewerTabState::Ready,
/// );
/// assert_eq!(tab.id(), id);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerTab {
    id: ViewerTabId,
    label: String,
    kind: ViewerTabKind,
    state: ViewerTabState,
}

impl ViewerTab {
    /// Creates a tab-strip entry from validated identity and closed state values.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
    ///
    /// let id = ViewerTabId::try_new(1).expect("positive id");
    /// let tab = ViewerTab::new(
    ///     id,
    ///     "Changes".into(),
    ///     ViewerTabKind::Snapshot,
    ///     ViewerTabState::Ready,
    /// );
    /// assert_eq!(tab.label(), "Changes");
    /// ```
    pub fn new(id: ViewerTabId, label: String, kind: ViewerTabKind, state: ViewerTabState) -> Self {
        Self {
            id,
            label,
            kind,
            state,
        }
    }

    /// Returns the validated tab identity.
    ///
    /// # Examples
    ///
    /// ```
    /// # use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
    /// # let id = ViewerTabId::try_new(1).expect("positive id");
    /// # let tab = ViewerTab::new(id, "Changes".into(), ViewerTabKind::Snapshot, ViewerTabState::Ready);
    /// assert_eq!(tab.id(), id);
    /// ```
    pub const fn id(&self) -> ViewerTabId {
        self.id
    }

    /// Returns the tab-strip label.
    ///
    /// # Examples
    ///
    /// ```
    /// # use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
    /// # let id = ViewerTabId::try_new(1).expect("positive id");
    /// # let tab = ViewerTab::new(id, "Changes".into(), ViewerTabKind::Snapshot, ViewerTabState::Ready);
    /// assert_eq!(tab.label(), "Changes");
    /// ```
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the tab is a snapshot or live view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
    /// # let id = ViewerTabId::try_new(1).expect("positive id");
    /// # let tab = ViewerTab::new(id, "Changes".into(), ViewerTabKind::Snapshot, ViewerTabState::Ready);
    /// assert_eq!(tab.kind(), ViewerTabKind::Snapshot);
    /// ```
    pub const fn kind(&self) -> ViewerTabKind {
        self.kind
    }

    /// Returns the tab's authoritative rendering state.
    ///
    /// # Examples
    ///
    /// ```
    /// # use domain::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
    /// # let id = ViewerTabId::try_new(1).expect("positive id");
    /// # let tab = ViewerTab::new(id, "Changes".into(), ViewerTabKind::Snapshot, ViewerTabState::Ready);
    /// assert_eq!(tab.state(), &ViewerTabState::Ready);
    /// ```
    pub const fn state(&self) -> &ViewerTabState {
        &self.state
    }
}
