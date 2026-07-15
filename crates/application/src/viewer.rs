use std::{collections::HashSet, sync::Arc};

pub use domain::viewer::{
    DiffDensity, DiffLayout, ParseRenderOptionError, RenderHistoryId, RenderOptions, Theme,
    ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState,
};

use crate::diffs::View;

/// Couples one ready tab with the diff view and options used to render it.
///
/// The shared [`View`] keeps session snapshots cheap to clone.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::Arc;
/// # use application::{diffs::View, viewer::{RenderOptions, ViewerTabId, ViewerTabKind, ViewerView}};
/// # fn load_view() -> View { unimplemented!() }
/// let tab_id = ViewerTabId::try_new(1).expect("positive id");
/// let view = ViewerView::new(tab_id, Arc::new(load_view()), RenderOptions::DEFAULT, ViewerTabKind::Snapshot);
/// assert_eq!(view.tab_id(), tab_id);
/// ```
#[derive(Debug, Clone)]
pub struct ViewerView {
    tab_id: ViewerTabId,
    view: Arc<View>,
    options: RenderOptions,
    kind: ViewerTabKind,
}

impl ViewerView {
    /// Creates the rendered content associated with one ready tab.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use application::{diffs::View, viewer::{RenderOptions, ViewerTabId, ViewerTabKind, ViewerView}};
    /// # fn load_view() -> View { unimplemented!() }
    /// let id = ViewerTabId::try_new(1).expect("positive id");
    /// let rendered = ViewerView::new(id, Arc::new(load_view()), RenderOptions::DEFAULT, ViewerTabKind::Snapshot);
    /// assert_eq!(rendered.tab_id(), id);
    /// ```
    pub fn new(
        tab_id: ViewerTabId,
        view: Arc<View>,
        options: RenderOptions,
        kind: ViewerTabKind,
    ) -> Self {
        Self {
            tab_id,
            view,
            options,
            kind,
        }
    }

    /// Returns the identity of the tab that owns this rendered view.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::{ViewerTabId, ViewerView};
    /// # fn rendered() -> ViewerView { unimplemented!() }
    /// assert_eq!(
    ///     rendered().tab_id(),
    ///     ViewerTabId::try_new(1).expect("positive id")
    /// );
    /// ```
    pub const fn tab_id(&self) -> ViewerTabId {
        self.tab_id
    }

    /// Returns the diff content rendered for the tab.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerView;
    /// # fn rendered() -> ViewerView { unimplemented!() }
    /// assert_eq!(rendered().view().repo_name, "git-tools");
    /// ```
    pub fn view(&self) -> &View {
        &self.view
    }

    /// Returns the layout and density used to render the diff.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::{RenderOptions, ViewerView};
    /// # fn rendered() -> ViewerView { unimplemented!() }
    /// assert_eq!(rendered().options(), RenderOptions::DEFAULT);
    /// ```
    pub const fn options(&self) -> RenderOptions {
        self.options
    }

    /// Returns whether the rendered content belongs to a snapshot or live tab.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::{ViewerTabKind, ViewerView};
    /// # fn rendered() -> ViewerView { unimplemented!() }
    /// assert_eq!(rendered().kind(), ViewerTabKind::Snapshot);
    /// ```
    pub const fn kind(&self) -> ViewerTabKind {
        self.kind
    }
}

/// Holds one stable-ID entry rendered in the viewer's recent-history list.
///
/// # Examples
///
/// ```
/// use application::viewer::{RenderHistoryId, ViewerHistoryEntry};
///
/// let id = RenderHistoryId::try_new(1).expect("positive id");
/// let entry = ViewerHistoryEntry::new(
///     id,
///     "Changes".into(),
///     "git-tools".into(),
///     "diff".into(),
///     "main..HEAD".into(),
///     "2026-07-11T00:00:00Z".into(),
/// );
/// assert_eq!(entry.id(), id);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerHistoryEntry {
    id: RenderHistoryId,
    title: String,
    repo_name: String,
    kind: String,
    range_label: String,
    rendered_at: String,
}

impl ViewerHistoryEntry {
    /// Creates a history entry from one validated persisted row identity.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::{RenderHistoryId, ViewerHistoryEntry};
    ///
    /// let id = RenderHistoryId::try_new(1).expect("positive id");
    /// let entry = ViewerHistoryEntry::new(
    ///     id,
    ///     "Changes".into(),
    ///     "git-tools".into(),
    ///     "diff".into(),
    ///     "main..HEAD".into(),
    ///     "2026-07-11T00:00:00Z".into(),
    /// );
    /// assert_eq!(entry.id(), id);
    /// ```
    pub fn new(
        id: RenderHistoryId,
        title: String,
        repo_name: String,
        kind: String,
        range_label: String,
        rendered_at: String,
    ) -> Self {
        Self {
            id,
            title,
            repo_name,
            kind,
            range_label,
            rendered_at,
        }
    }

    /// Returns the stable persisted-row identity used to reopen this render.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::{RenderHistoryId, ViewerHistoryEntry};
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(
    ///     entry().id(),
    ///     RenderHistoryId::try_new(1).expect("positive id")
    /// );
    /// ```
    pub const fn id(&self) -> RenderHistoryId {
        self.id
    }

    /// Returns the human-readable render title.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerHistoryEntry;
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(entry().title(), "Changes");
    /// ```
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the repository name displayed for the render.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerHistoryEntry;
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(entry().repo_name(), "git-tools");
    /// ```
    pub fn repo_name(&self) -> &str {
        &self.repo_name
    }

    /// Returns the recipe-kind label displayed for the render.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerHistoryEntry;
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(entry().kind(), "diff");
    /// ```
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the range label displayed for the render.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerHistoryEntry;
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(entry().range_label(), "main..HEAD");
    /// ```
    pub fn range_label(&self) -> &str {
        &self.range_label
    }

    /// Returns the persisted rendering timestamp.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use application::viewer::ViewerHistoryEntry;
    /// # fn entry() -> ViewerHistoryEntry { unimplemented!() }
    /// assert_eq!(entry().rendered_at(), "2026-07-11T00:00:00Z");
    /// ```
    pub fn rendered_at(&self) -> &str {
        &self.rendered_at
    }
}

/// Holds the validated rendering and color choices applied to the viewer document.
///
/// # Examples
///
/// ```
/// use application::viewer::{RenderOptions, Theme, ViewerSettings};
///
/// let settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark);
/// assert_eq!(settings.theme(), Theme::Dark);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerSettings {
    options: RenderOptions,
    theme: Theme,
}

impl ViewerSettings {
    /// Creates settings from validated rendering options and a closed theme value.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::{RenderOptions, Theme, ViewerSettings};
    ///
    /// let settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark);
    /// assert_eq!(settings.theme(), Theme::Dark);
    /// ```
    pub const fn new(options: RenderOptions, theme: Theme) -> Self {
        Self { options, theme }
    }

    /// Returns the viewer's diff rendering options.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::{RenderOptions, Theme, ViewerSettings};
    ///
    /// let settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark);
    /// assert_eq!(settings.options(), RenderOptions::DEFAULT);
    /// ```
    pub const fn options(&self) -> RenderOptions {
        self.options
    }

    /// Returns the viewer's selected color theme.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::{RenderOptions, Theme, ViewerSettings};
    ///
    /// let settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Hearth);
    /// assert_eq!(settings.theme(), Theme::Hearth);
    /// ```
    pub const fn theme(&self) -> Theme {
        self.theme
    }
}

/// Describes a cross-field invariant rejected while constructing a [`ViewerDocument`].
///
/// # Examples
///
/// ```
/// use application::viewer::{
///     RenderOptions, Theme, ViewerDocument, ViewerDocumentError, ViewerSettings, ViewerTabId,
/// };
///
/// let missing = ViewerTabId::try_new(1).expect("positive id");
/// let error = ViewerDocument::new(
///     vec![],
///     Some(missing),
///     None,
///     vec![],
///     ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark),
/// )
/// .expect_err("active tab must exist");
/// assert_eq!(
///     error,
///     ViewerDocumentError::ActiveTabMissing { tab_id: missing }
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewerDocumentError {
    /// Reports an identity assigned to more than one tab-strip entry.
    #[error("viewer tab {tab_id} appears more than once in the tab strip")]
    DuplicateTabId { tab_id: ViewerTabId },
    /// Reports an active identity absent from the document's tab strip.
    #[error("active viewer tab {tab_id} is absent from the tab strip")]
    ActiveTabMissing { tab_id: ViewerTabId },
    /// Reports rendered content owned by a tab other than the active one.
    #[error("active view belongs to tab {view_tab_id}, not active tab {active_tab_id}")]
    ActiveViewMismatch {
        active_tab_id: ViewerTabId,
        view_tab_id: ViewerTabId,
    },
    /// Reports rendered content whose snapshot/live kind differs from its owning tab.
    #[error("active view kind {view_kind:?} does not match active tab {tab_id} kind {tab_kind:?}")]
    ActiveViewKindMismatch {
        tab_id: ViewerTabId,
        tab_kind: ViewerTabKind,
        view_kind: ViewerTabKind,
    },
    /// Reports a ready active tab with no corresponding rendered content.
    #[error("ready active tab {tab_id} has no rendered view")]
    ReadyTabMissingView { tab_id: ViewerTabId },
    /// Reports rendered content attached to a broken or error active tab.
    #[error("non-ready active tab {tab_id} unexpectedly has a rendered view")]
    NonReadyTabHasView { tab_id: ViewerTabId },
}

/// Holds a viewer page whose active identity, tab state, and optional view agree.
///
/// An active broken or error tab intentionally has no active rendered view.
///
/// # Examples
///
/// ```
/// use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
///
/// let document = ViewerDocument::new(
///     vec![],
///     None,
///     None,
///     vec![],
///     ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark),
/// )
/// .expect("empty document is valid");
/// assert!(document.active_tab().is_none());
/// ```
#[derive(Debug, Clone)]
pub struct ViewerDocument {
    tabs: Vec<ViewerTab>,
    active_tab_id: Option<ViewerTabId>,
    active_view: Option<ViewerView>,
    history: Vec<ViewerHistoryEntry>,
    settings: ViewerSettings,
}

impl ViewerDocument {
    /// Creates a document only when its active tab identity, state, and rendered view agree.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerDocumentError`] when tab identities are duplicated, the active identity is
    /// missing, the active rendered view belongs to another tab, its kind differs from the tab
    /// kind, or the active tab state disagrees with view presence.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    ///
    /// let document = ViewerDocument::new(
    ///     vec![],
    ///     None,
    ///     None,
    ///     vec![],
    ///     ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark),
    /// )
    /// .expect("an empty viewer is valid");
    /// assert!(document.tabs().is_empty());
    /// ```
    pub fn new(
        tabs: Vec<ViewerTab>,
        active_tab_id: Option<ViewerTabId>,
        active_view: Option<ViewerView>,
        history: Vec<ViewerHistoryEntry>,
        settings: ViewerSettings,
    ) -> Result<Self, ViewerDocumentError> {
        let mut tab_ids = HashSet::with_capacity(tabs.len());
        for tab in &tabs {
            if !tab_ids.insert(tab.id()) {
                return Err(ViewerDocumentError::DuplicateTabId { tab_id: tab.id() });
            }
        }

        let active_tab = active_tab_id
            .map(|tab_id| {
                tabs.iter()
                    .find(|tab| tab.id() == tab_id)
                    .ok_or(ViewerDocumentError::ActiveTabMissing { tab_id })
            })
            .transpose()?;

        if let (Some(tab), Some(view)) = (active_tab, &active_view) {
            if tab.id() != view.tab_id() {
                return Err(ViewerDocumentError::ActiveViewMismatch {
                    active_tab_id: tab.id(),
                    view_tab_id: view.tab_id(),
                });
            }
            if tab.kind() != view.kind() {
                return Err(ViewerDocumentError::ActiveViewKindMismatch {
                    tab_id: tab.id(),
                    tab_kind: tab.kind(),
                    view_kind: view.kind(),
                });
            }
        }

        match (active_tab, active_view.as_ref()) {
            (Some(tab), None) if matches!(tab.state(), ViewerTabState::Ready) => {
                return Err(ViewerDocumentError::ReadyTabMissingView { tab_id: tab.id() });
            }
            (Some(tab), Some(_))
                if matches!(
                    tab.state(),
                    ViewerTabState::Broken { .. } | ViewerTabState::Error { .. }
                ) =>
            {
                return Err(ViewerDocumentError::NonReadyTabHasView { tab_id: tab.id() });
            }
            (None, Some(view)) => {
                return Err(ViewerDocumentError::ActiveTabMissing {
                    tab_id: view.tab_id(),
                });
            }
            _ => {}
        }

        Ok(Self {
            tabs,
            active_tab_id,
            active_view,
            history,
            settings,
        })
    }

    /// Returns the ordered tab-strip entries.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)).expect("valid viewer");
    /// assert!(document.tabs().is_empty());
    /// ```
    pub fn tabs(&self) -> &[ViewerTab] {
        &self.tabs
    }

    /// Returns the active tab identity independently of rendered-view availability.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)).expect("valid viewer");
    /// assert_eq!(document.active_tab_id(), None);
    /// ```
    pub const fn active_tab_id(&self) -> Option<ViewerTabId> {
        self.active_tab_id
    }

    /// Returns the tab-strip entry selected as active.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)).expect("valid viewer");
    /// assert!(document.active_tab().is_none());
    /// ```
    pub fn active_tab(&self) -> Option<&ViewerTab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter().find(|tab| tab.id() == id))
    }

    /// Returns rendered content only when the active tab is ready.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)).expect("valid viewer");
    /// assert!(document.active_view().is_none());
    /// ```
    pub fn active_view(&self) -> Option<&ViewerView> {
        self.active_view.as_ref()
    }

    /// Returns recent renders in the order supplied by the application query.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)).expect("valid viewer");
    /// assert!(document.history().is_empty());
    /// ```
    pub fn history(&self) -> &[ViewerHistoryEntry] {
        &self.history
    }

    /// Returns the validated settings rendered into this document.
    ///
    /// # Examples
    ///
    /// ```
    /// # use application::viewer::{RenderOptions, Theme, ViewerDocument, ViewerSettings};
    /// # let document = ViewerDocument::new(vec![], None, None, vec![], ViewerSettings::new(RenderOptions::DEFAULT, Theme::Hearth)).expect("valid viewer");
    /// assert_eq!(document.settings().theme(), Theme::Hearth);
    /// ```
    pub const fn settings(&self) -> &ViewerSettings {
        &self.settings
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        RenderOptions, Theme, ViewerDocument, ViewerDocumentError, ViewerSettings, ViewerTab,
        ViewerTabId, ViewerTabKind, ViewerTabState, ViewerView,
    };
    use crate::diffs::{Cmd, Foot, View};

    fn tab_id(raw: u64) -> ViewerTabId {
        ViewerTabId::try_new(raw).expect("positive tab id")
    }

    fn view() -> Arc<View> {
        Arc::new(View {
            repo_name: "git-tools".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "origin/main".into(),
            commits: vec![],
            files: vec![],
            title: "diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
            theme: None,
            exclusions: None,
        })
    }

    fn settings() -> ViewerSettings {
        ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)
    }

    #[test]
    fn broken_tab_can_be_active_without_a_rendered_view() {
        let id = tab_id(1);
        let tab = ViewerTab::new(
            id,
            "broken".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Broken {
                code: "missing".into(),
                reason: "artifact disappeared".into(),
            },
        );

        let document = ViewerDocument::new(vec![tab], Some(id), None, vec![], settings())
            .expect("broken active tab is valid without a view");

        assert_eq!(document.active_tab_id(), Some(id));
        assert!(document.active_view().is_none());
    }

    #[test]
    fn ready_active_tab_requires_its_rendered_view() {
        let id = tab_id(1);
        let tab = ViewerTab::new(
            id,
            "ready".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Ready,
        );

        let error = ViewerDocument::new(vec![tab], Some(id), None, vec![], settings())
            .expect_err("ready active tab requires its view");

        assert_eq!(
            error,
            ViewerDocumentError::ReadyTabMissingView { tab_id: id }
        );
    }

    #[test]
    fn duplicate_tab_id_is_rejected_before_active_lookup() {
        let id = tab_id(1);
        let duplicate = || {
            ViewerTab::new(
                id,
                "duplicate".into(),
                ViewerTabKind::Snapshot,
                ViewerTabState::Error {
                    reason: "failed".into(),
                },
            )
        };

        let error = ViewerDocument::new(
            vec![duplicate(), duplicate()],
            Some(tab_id(2)),
            None,
            vec![],
            settings(),
        )
        .expect_err("duplicate identities reject before missing active identity");

        assert_eq!(error, ViewerDocumentError::DuplicateTabId { tab_id: id });
    }

    #[test]
    fn active_view_must_belong_to_the_active_tab() {
        let active_id = tab_id(1);
        let view_id = tab_id(2);
        let tabs = vec![
            ViewerTab::new(
                active_id,
                "one".into(),
                ViewerTabKind::Snapshot,
                ViewerTabState::Ready,
            ),
            ViewerTab::new(
                view_id,
                "two".into(),
                ViewerTabKind::Snapshot,
                ViewerTabState::Ready,
            ),
        ];
        let active_view = ViewerView::new(
            view_id,
            view(),
            RenderOptions::DEFAULT,
            ViewerTabKind::Snapshot,
        );

        let error =
            ViewerDocument::new(tabs, Some(active_id), Some(active_view), vec![], settings())
                .expect_err("mismatched active view rejects");

        assert_eq!(
            error,
            ViewerDocumentError::ActiveViewMismatch {
                active_tab_id: active_id,
                view_tab_id: view_id,
            }
        );
    }

    #[test]
    fn active_view_kind_must_match_the_active_tab_kind() {
        let id = tab_id(1);
        let tab = ViewerTab::new(
            id,
            "snapshot".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Ready,
        );
        let active_view = ViewerView::new(id, view(), RenderOptions::DEFAULT, ViewerTabKind::Live);

        let result =
            ViewerDocument::new(vec![tab], Some(id), Some(active_view), vec![], settings());

        assert_eq!(
            result.expect_err("mismatched tab and view kinds must reject"),
            ViewerDocumentError::ActiveViewKindMismatch {
                tab_id: id,
                tab_kind: ViewerTabKind::Snapshot,
                view_kind: ViewerTabKind::Live,
            }
        );
    }
}
