use dioxus::prelude::*;
#[cfg(feature = "component-preview")]
use lucide_dioxus::FileText;
use lucide_dioxus::{ChevronDown, ChevronUp, Files, Search, X};

#[cfg(feature = "component-preview")]
use crate::views::diffs::search_keybindings::SEARCH_TEXT_IN_FILE_KEY_BINDING;
use crate::{
    shared::ui::{
        Button, ButtonSize, ButtonState, ButtonVariant, KeyboardShortcut, TextInput,
        TextInputLabelVisibility,
    },
    views::diffs::search_keybindings::SEARCH_TEXT_IN_ALL_FILES_KEY_BINDING,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::views::diffs) enum DiffSearchScope {
    #[cfg(feature = "component-preview")]
    ActiveFile {
        path: String,
    },
    AllFiles,
}

impl DiffSearchScope {
    const fn label(&self) -> &'static str {
        match self {
            #[cfg(feature = "component-preview")]
            Self::ActiveFile { .. } => "This file",
            Self::AllFiles => "All files",
        }
    }

    fn context(&self) -> &str {
        match self {
            #[cfg(feature = "component-preview")]
            Self::ActiveFile { path } => path,
            Self::AllFiles => "Search code across the complete diff",
        }
    }

    const fn placeholder(&self) -> &'static str {
        match self {
            #[cfg(feature = "component-preview")]
            Self::ActiveFile { .. } => "Search code in this file...",
            Self::AllFiles => "Search code in all files...",
        }
    }

    fn shortcut(&self) -> Vec<&'static str> {
        match self {
            #[cfg(feature = "component-preview")]
            Self::ActiveFile { .. } => SEARCH_TEXT_IN_FILE_KEY_BINDING.to_vec(),
            Self::AllFiles => SEARCH_TEXT_IN_ALL_FILES_KEY_BINDING.to_vec(),
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
    #[props(default)] show_shortcut: bool,
    onquerychange: EventHandler<String>,
    onprevious: EventHandler<()>,
    onnext: EventHandler<()>,
    onclose: EventHandler<()>,
) -> Element {
    let navigation_state = if navigation_enabled {
        ButtonState::Enabled
    } else {
        ButtonState::Disabled
    };
    let aria_label = format!("Find code in {}", scope.label().to_lowercase());

    rsx! {
        section {
            class: "absolute top-3 right-5 z-20 w-[min(34rem,calc(100%-1.5rem))] overflow-hidden rounded-panel border border-line-2 bg-surface shadow-floating tablet:right-3 mobile:top-1 mobile:right-1",
            role: "search",
            aria_label,
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
            header { class: "flex min-w-0 items-center gap-2 border-b border-line bg-surface-2 px-3 py-2",
                span {
                    class: "flex size-6 flex-none items-center justify-center rounded-sm border border-line bg-sunk text-acc",
                    aria_hidden: "true",
                    match &scope {
                        #[cfg(feature = "component-preview")]
                        DiffSearchScope::ActiveFile { .. } => rsx! {
                            FileText { size: 14 }
                        },
                        DiffSearchScope::AllFiles => rsx! {
                            Files { size: 14 }
                        },
                    }
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs font-semibold text-ink", "{scope.label()}" }
                    span { class: "block truncate text-[0.6875rem] text-ink-3", "{scope.context()}" }
                }
                if show_shortcut {
                    KeyboardShortcut { keys: scope.shortcut() }
                }
            }
            div { class: "grid grid-cols-[minmax(0,1fr)_auto] gap-x-2 gap-y-1.5 p-2",
                div { class: "relative min-w-0",
                    span {
                        class: "pointer-events-none absolute top-1/2 left-2.5 z-2 -translate-y-1/2 text-ink-3",
                        aria_hidden: "true",
                        Search { size: 15 }
                    }
                    TextInput {
                        id: input_id,
                        label: "Search code",
                        label_visibility: TextInputLabelVisibility::Hidden,
                        class: "h-9 py-2 pr-2 pl-8",
                        value: query,
                        maxlength,
                        placeholder: scope.placeholder(),
                        oninput: move |event: FormEvent| onquerychange.call(event.value()),
                    }
                }
                div { class: "flex items-center gap-1",
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        state: navigation_state,
                        aria_label: "Previous match",
                        title: "Previous match (Shift+Enter)",
                        onclick: move |_| onprevious.call(()),
                        span { aria_hidden: "true",
                            ChevronUp { size: 16 }
                        }
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        state: navigation_state,
                        aria_label: "Next match",
                        title: "Next match (Enter)",
                        onclick: move |_| onnext.call(()),
                        span { aria_hidden: "true",
                            ChevronDown { size: 16 }
                        }
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: "Close search",
                        title: "Close search (Escape)",
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
                scope: DiffSearchScope::ActiveFile {
                    path: "src/main.rs".to_owned(),
                },
                query: "needle",
                status_message: "2 matches",
                navigation_enabled: true,
                maxlength: None,
                show_shortcut: true,
                onquerychange: move |_| {},
                onprevious: move |()| {},
                onnext: move |()| {},
                onclose: move |()| {},
            }
        }
    }

    #[test]
    fn renders_the_scope_and_its_shortcut() {
        let html = dioxus_ssr::render_element(rsx! {
            SearchBarTestView {}
        });

        assert!(html.contains("This file"));
        assert!(html.contains("src/main.rs"));
        assert!(html.contains(">Ctrl<"));
        assert!(html.contains(">F<"));
    }
}
