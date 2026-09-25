//! Owns the language the desktop viewer displays before and after its settings load.

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;

use crate::shared::{browser, i18n};

/// The language the desktop viewer displays, or `None` while it is unknown.
///
/// A start begins with the language stored when the viewer last displayed one,
/// so copy renders in it before the server's settings arrive. Without a stored
/// language the viewer stays hidden until the settings load or the first
/// connection attempt fails, so copy never switches language on screen.
///
/// Shell actions reach the owner through the current scope's context: they run
/// in handlers and tasks of the viewer's components, and component tests that
/// render without the desktop root have no owner to update.
#[derive(Clone, Copy)]
pub(crate) struct DisplayedLanguage(Signal<Option<ViewerLanguage>>);

impl DisplayedLanguage {
    /// Displays the configured `language` and stores it for the next start.
    pub(crate) fn show_configured(language: ViewerLanguage) {
        let Some(Self(mut displayed)) = try_consume_context() else {
            return;
        };
        if *displayed.peek() != Some(language) {
            displayed.set(Some(language));
            browser::store_viewer_language(language);
        }
    }

    /// Displays the default language when the viewer cannot learn its setting.
    pub(crate) fn show_default_when_unknown() {
        let Some(Self(mut displayed)) = try_consume_context() else {
            return;
        };
        if displayed.peek().is_none() {
            displayed.set(Some(ViewerLanguage::default()));
        }
    }

    /// Reports whether a language is known, subscribing the caller to that change.
    pub(crate) fn is_known(self) -> bool {
        self.0.read().is_some()
    }
}

/// Provides the displayed language to the viewer and to [`i18n::use_language`].
pub(super) fn use_displayed_language_provider() {
    let displayed = use_signal(browser::stored_viewer_language);
    let language = use_memo(move || displayed().unwrap_or_default());
    i18n::use_language_provider(language.into());
    use_effect(move || browser::apply_document_language(language()));
    use_context_provider(|| DisplayedLanguage(displayed));
}
