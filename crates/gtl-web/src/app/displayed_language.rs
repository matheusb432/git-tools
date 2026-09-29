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
/// The shared settings owner supplies changes. The shell supplies the initial
/// language until those settings load. Component previews can supply their own
/// language without the desktop root.
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
    let settings = crate::app::user_settings::use_settings_selection_provider();
    let displayed = use_signal(browser::stored_viewer_language);
    let language = use_memo(move || {
        settings().map_or_else(
            || displayed().unwrap_or_default(),
            |settings| settings.language,
        )
    });
    i18n::use_language_provider(language.into());
    use_effect(move || {
        browser::apply_document_language(language());
        if settings.peek().is_some() {
            browser::store_viewer_language(language());
        }
    });
    use_context_provider(|| DisplayedLanguage(displayed));
}
