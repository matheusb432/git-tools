//! Browser-facing identifiers shared by the Dioxus client and its end-to-end tests.

/// A stable browser test identifier and its CSS selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TestId {
    value: &'static str,
    selector: &'static str,
}

impl TestId {
    const fn new(value: &'static str, selector: &'static str) -> Self {
        Self { value, selector }
    }

    /// Returns the value used in a `data-testid` attribute.
    pub const fn value(self) -> &'static str {
        self.value
    }

    /// Returns the CSS selector for the matching `data-testid` attribute.
    pub const fn selector(self) -> &'static str {
        self.selector
    }
}

macro_rules! define_test_id {
    ($name:ident, $value:literal) => {
        pub const $name: TestId = TestId::new($value, concat!("[data-testid=\"", $value, "\"]"));
    };
}

define_test_id!(CHANGED_FILES_PANEL, "changed-files-panel");
define_test_id!(HISTORY_ENTRY_OPEN, "history-entry-open");
define_test_id!(LIVE_VIEW_REFRESH, "live-view-refresh");
define_test_id!(VIEWER_HISTORY_OPEN, "viewer-history-open");
define_test_id!(VIEWER_MENU_TRIGGER, "viewer-menu-trigger");
define_test_id!(VIEWER_TAB_CLOSE, "viewer-tab-close");
