use std::num::NonZeroUsize;

use dioxus::prelude::*;
use dx_story::catalog::{Catalog, CatalogConfig};
use gtl_wire::viewer::ViewerTheme;

use crate::shared::{browser, ui::ViewerThemePicker};

const FAVICON: Asset = asset!("/src/app/assets/app-icon.svg");
const PREVIEW_CSS: Asset = asset!("/assets/component-preview.css");
const CATALOG_CONFIG: CatalogConfig = CatalogConfig::new("Component catalog")
    .with_story_sets_per_page(match NonZeroUsize::new(15) {
        Some(value) => value,
        None => NonZeroUsize::MIN,
    })
    .with_canvas_class("font-mono")
    .with_sidebar_footer(SidebarThemePicker);

#[component]
pub(super) fn App() -> Element {
    let theme = use_signal(ViewerTheme::default);
    use_context_provider(|| theme);
    let selected_theme = theme();
    use_effect(use_reactive((&selected_theme,), move |(selected_theme,)| {
        browser::apply_theme(selected_theme.as_str());
    }));

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: PREVIEW_CSS, blocking: "render" }
        Catalog { config: CATALOG_CONFIG }
    }
}

#[component]
fn SidebarThemePicker() -> Element {
    let mut theme = use_context::<Signal<ViewerTheme>>();

    rsx! {
        ViewerThemePicker {
            theme: theme(),
            disabled: false,
            onthemechange: move |selected_theme| theme.set(selected_theme),
        }
    }
}
