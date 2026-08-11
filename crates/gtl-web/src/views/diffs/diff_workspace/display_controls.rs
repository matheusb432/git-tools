use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerDiffDensity, ViewerDiffLayout, ViewerPreferences,
};
use lucide_dioxus::{GitCommitHorizontal, ListFilter, PanelLeft};

use super::MobilePanel;
use crate::shared::ui::{Button, ButtonSize, ButtonState, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayControlGroupKind {
    Density,
    Layout,
}

impl DisplayControlGroupKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Density => "View",
            Self::Layout => "Layout",
        }
    }

    fn options(self, preferences: ViewerPreferences) -> [DisplayControlOption; 2] {
        match self {
            Self::Density => [
                DisplayControlOption {
                    label: "Changes",
                    selected: preferences.render_options.density == ViewerDiffDensity::Compact,
                    preference: SetViewerPreference::Density(ViewerDiffDensity::Compact),
                },
                DisplayControlOption {
                    label: "Full file",
                    selected: preferences.render_options.density == ViewerDiffDensity::Full,
                    preference: SetViewerPreference::Density(ViewerDiffDensity::Full),
                },
            ],
            Self::Layout => [
                DisplayControlOption {
                    label: "Unified",
                    selected: preferences.render_options.layout == ViewerDiffLayout::Unified,
                    preference: SetViewerPreference::Layout(ViewerDiffLayout::Unified),
                },
                DisplayControlOption {
                    label: "Side by side",
                    selected: preferences.render_options.layout == ViewerDiffLayout::Split,
                    preference: SetViewerPreference::Layout(ViewerDiffLayout::Split),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayControlOption {
    label: &'static str,
    selected: bool,
    preference: SetViewerPreference,
}

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
            DisplayControlGroup {
                kind: DisplayControlGroupKind::Layout,
                preferences,
                onpreference,
            }
            DisplayControlGroup {
                kind: DisplayControlGroupKind::Density,
                preferences,
                onpreference,
            }
            if is_live {
                LiveViewActions {
                    pending,
                    delete_trigger_id,
                    onrefresh,
                    ondelete,
                }
            }
        }
    }
}

#[component]
fn DisplayControlGroup(
    kind: DisplayControlGroupKind,
    preferences: ViewerPreferences,
    onpreference: EventHandler<SetViewerPreference>,
) -> Element {
    let label = kind.label();
    let options = kind.options(preferences);

    rsx! {
        div {
            class: "flex items-center gap-1",
            role: "group",
            aria_label: label,
            DisplayControlLabel { label }
            for option in options {
                DisplayOptionButton {
                    label: option.label,
                    selected: option.selected,
                    preference: option.preference,
                    onpreference,
                }
            }
        }
    }
}

#[component]
fn DisplayControlLabel(label: &'static str) -> Element {
    rsx! {
        span { class: "mr-1 font-bold tracking-wider text-ink-3 uppercase", "{label}" }
    }
}

#[component]
fn DisplayOptionButton(
    label: &'static str,
    selected: bool,
    preference: SetViewerPreference,
    onpreference: EventHandler<SetViewerPreference>,
) -> Element {
    rsx! {
        Button {
            size: ButtonSize::Small,
            variant: if selected { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
            aria_pressed: selected.to_string(),
            onclick: move |_| onpreference.call(preference),
            "{label}"
        }
    }
}

#[component]
fn LiveViewActions(
    pending: bool,
    delete_trigger_id: String,
    onrefresh: EventHandler<MouseEvent>,
    ondelete: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
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
            MobilePanelIcon { icon }
            "{label}"
        }
    }
}

#[component]
fn MobilePanelIcon(icon: MobilePanel) -> Element {
    rsx! {
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
    }
}
