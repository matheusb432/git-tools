use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerDiffDensity, ViewerDiffLayout, ViewerPreferences,
};
use lucide_dioxus::{GitCommitHorizontal, ListFilter, PanelLeft};

use super::MobilePanel;
use crate::shared::ui::{Button, ButtonSize, ButtonState, ButtonVariant};

#[component]
pub(super) fn DisplayControls(
    preferences: ViewerPreferences,
    pending: bool,
    is_live: bool,
    delete_trigger_id: String,
    onpreference: EventHandler<SetViewerPreference>,
    onrefresh: EventHandler<MouseEvent>,
    ondelete: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "flex min-w-0 flex-wrap items-center gap-3",
            div {
                class: "flex items-center gap-1",
                role: "group",
                aria_label: "Layout",
                span { class: "mr-1 font-bold tracking-wider text-ink-3 uppercase", "Layout" }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Unified { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Unified).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Unified)),
                    "Unified"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Split { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Split).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Split)),
                    "Side by side"
                }
            }
            div {
                class: "flex items-center gap-1",
                role: "group",
                aria_label: "View",
                span { class: "mr-1 font-bold tracking-wider text-ink-3 uppercase", "View" }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Compact { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Compact).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Compact)),
                    "Changes"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Full { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Full).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Full)),
                    "Full file"
                }
            }
            if is_live {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Ghost,
                    state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
                    onclick: onrefresh,
                    "Refresh"
                }
                Button {
                    id: delete_trigger_id,
                    class: "ml-2",
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Destructive,
                    onclick: ondelete,
                    "Delete live view"
                }
            }
        }
    }
}

#[component]
pub(super) fn MobilePanelButton(
    id: String,
    label: String,
    icon: MobilePanel,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        Button {
            id,
            size: ButtonSize::Small,
            variant: ButtonVariant::Outline,
            onclick,
            span { aria_hidden: "true",
                match icon {
                    MobilePanel::Display => rsx! {
                        ListFilter { size: 14 }
                    },
                    MobilePanel::Files => rsx! {
                        PanelLeft { size: 14 }
                    },
                    MobilePanel::Commits => rsx! {
                        GitCommitHorizontal { size: 14 }
                    },
                }
            }
            "{label}"
        }
    }
}
