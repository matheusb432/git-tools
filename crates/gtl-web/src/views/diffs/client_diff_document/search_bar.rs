use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use lucide_dioxus::{ChevronDown, ChevronUp, Search, X};

use crate::shared::{
    i18n::{t, use_language},
    ui::{
        Button, ButtonSize, ButtonState, ButtonVariant, FieldLabelVisibility, SearchPanel,
        TextInput,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::views::diffs) enum DiffSearchScope {
    AllFiles,
}

impl DiffSearchScope {
    fn search_label(&self, language: ViewerLanguage) -> String {
        match self {
            Self::AllFiles => t!(language, "diff-search-all-files-label"),
        }
    }

    fn placeholder(&self, language: ViewerLanguage) -> String {
        match self {
            Self::AllFiles => t!(language, "diff-search-all-files-placeholder"),
        }
    }
}

#[component]
pub(in crate::views::diffs) fn DiffSearchBar(
    input_id: String,
    scope: DiffSearchScope,
    query: String,
    status_message: String,
    navigation_enabled: bool,
    maxlength: Option<String>,
    onquerychange: EventHandler<String>,
    onprevious: EventHandler<()>,
    onnext: EventHandler<()>,
    onclose: EventHandler<()>,
) -> Element {
    let language = use_language();
    let navigation_state = if navigation_enabled {
        ButtonState::Enabled
    } else {
        ButtonState::Disabled
    };
    let aria_label = scope.search_label(language);

    rsx! {
        SearchPanel {
            label: aria_label,
            onkeydown: move |event: KeyboardEvent| {
                match event.key() {
                    Key::Escape => {
                        event.prevent_default();
                        onclose.call(());
                    }
                    Key::Enter => {
                        event.prevent_default();
                        if event.modifiers().shift() {
                            onprevious.call(());
                        } else {
                            onnext.call(());
                        }
                    }
                    _ => {}
                }
            },
            div { class: "diff-search-layout gap-x-2 gap-y-1.5 p-2",
                div { class: "relative min-w-0",
                    span {
                        class: "pointer-events-none absolute top-1/2 left-2.5 z-2 -translate-y-1/2 text-ink-3",
                        aria_hidden: "true",
                        Search { size: 15 }
                    }
                    TextInput {
                        id: input_id,
                        label: t!(language, "diff-search-code"),
                        label_visibility: FieldLabelVisibility::Hidden,
                        class: "h-9 py-2 pr-2 pl-8",
                        value: query,
                        maxlength,
                        placeholder: scope.placeholder(language),
                        oninput: move |event: FormEvent| onquerychange.call(event.value()),
                    }
                }
                div { class: "flex items-center gap-1",
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        state: navigation_state,
                        aria_label: t!(language, "diff-search-previous"),
                        title: t!(language, "diff-search-previous-title"),
                        onclick: move |_| onprevious.call(()),
                        span { aria_hidden: "true",
                            ChevronUp { size: 16 }
                        }
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        state: navigation_state,
                        aria_label: t!(language, "diff-search-next"),
                        title: t!(language, "diff-search-next-title"),
                        onclick: move |_| onnext.call(()),
                        span { aria_hidden: "true",
                            ChevronDown { size: 16 }
                        }
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: t!(language, "diff-search-close"),
                        title: t!(language, "diff-search-close-title"),
                        onclick: move |_| onclose.call(()),
                        span { aria_hidden: "true",
                            X { size: 16 }
                        }
                    }
                }
                p {
                    class: "col-[1/-1] min-h-4 px-0.5 text-xs text-ink-3",
                    role: "status",
                    aria_live: "polite",
                    "{status_message}"
                }
            }
        }
    }
}

#[cfg(all(test, feature = "component-preview"))]
mod tests {
    use dioxus::prelude::*;

    use super::{DiffSearchBar, DiffSearchScope};

    #[component]
    fn SearchBarTestView() -> Element {
        rsx! {
            DiffSearchBar {
                input_id: "search",
                scope: DiffSearchScope::AllFiles,
                query: "needle",
                status_message: "2 matches",
                navigation_enabled: true,
                maxlength: None,
                onquerychange: move |_| {},
                onprevious: move |()| {},
                onnext: move |()| {},
                onclose: move |()| {},
            }
        }
    }

    #[test]
    fn renders_the_workspace_scope_without_a_shortcut_hint() {
        let html = dioxus_ssr::render_element(rsx! {
            SearchBarTestView {}
        });

        assert!(html.contains("Search code in all files..."));
        assert!(!html.contains("<header"));
        assert!(!html.contains("<kbd"));
    }
}
