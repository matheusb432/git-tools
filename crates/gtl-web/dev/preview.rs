use std::num::NonZeroUsize;

use dioxus::prelude::*;
use dx_story::catalog::{Catalog, CatalogConfig};
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::ViewerTheme;

use crate::shared::{
    browser,
    i18n::{language_endonym, t, use_language, use_language_provider},
    ui::{Select, SelectOption, ViewerThemePicker},
};

const FAVICON: Asset = asset!("/src/app/assets/app-icon.svg");
const PREVIEW_CSS: Asset = asset!("/assets/component-preview.css");
const CATALOG_CONFIG: CatalogConfig = CatalogConfig::new("Component catalog")
    .with_story_sets_per_page(match NonZeroUsize::new(15) {
        Some(value) => value,
        None => NonZeroUsize::MIN,
    })
    .with_canvas_class("font-mono")
    .with_sidebar_footer(SidebarPreferences);

#[component]
pub(super) fn App() -> Element {
    let theme = use_signal(ViewerTheme::default);
    use_context_provider(|| theme);
    let selected_theme = theme();
    use_effect(use_reactive((&selected_theme,), move |(selected_theme,)| {
        browser::apply_theme(selected_theme.as_str());
    }));
    let language = use_signal(ViewerLanguage::default);
    use_context_provider(|| language);
    use_language_provider(language.into());
    use_effect(move || browser::apply_document_language(language()));

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: PREVIEW_CSS, blocking: "render" }
        Catalog { config: CATALOG_CONFIG }
    }
}

#[component]
fn SidebarPreferences() -> Element {
    let mut theme = use_context::<Signal<ViewerTheme>>();
    let mut language_setting = use_context::<Signal<ViewerLanguage>>();
    let language = use_language();

    rsx! {
        ViewerThemePicker {
            theme: theme(),
            disabled: false,
            onthemechange: move |selected_theme| theme.set(selected_theme),
        }
        Select {
            id: "component-preview-language",
            aria_label: t!(language, "settings-language"),
            value: language_setting().as_str(),
            options: ViewerLanguage::ALL
                .iter()
                .map(|language| SelectOption::new(
                    language.as_str(),
                    language_endonym(*language),
                ))
                .collect(),
            error: None,
            onchange: move |event: FormEvent| {
                if let Ok(selected) = event.value().parse() {
                    language_setting.set(selected);
                }
            },
        }
    }
}
