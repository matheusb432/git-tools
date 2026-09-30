use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerViewIdentity, push::CreateViewerPush};
use lucide_dioxus::{ChevronDown, ChevronLeft, ChevronRight, ChevronUp, X};

use crate::{
    app::{
        application_layout::{ViewerContext, ViewerShellLoad},
        application_navigation::{ViewerTabDirection, ViewerTabNavigation, ViewerTabStep},
    },
    shared::{
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonState, ButtonVariant},
    },
    views::{
        diffs::presentation::DiffPresentation,
        push::{
            PushButton, PushButtonPlacement, availability::use_view_push_availability,
            use_diff_push_shortcut,
        },
    },
};

#[component]
pub(super) fn ReviewActions(
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
) -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigation = use_context::<ViewerTabNavigation>();
    let mut collapsed = use_context::<DiffPresentation>().review_dock_collapsed;
    let (available, title, unpushed) = use_view_push_availability(identity, disabled);
    use_diff_push_shortcut(
        CreateViewerPush::View {
            identity: identity(),
        },
        !available,
    );
    let tab_id = identity().tab_id;
    let (pinned, tab_count) = viewer.shell().with(|shell| match shell {
        ViewerShellLoad::Ready(shell) => (
            shell.tabs.iter().any(|tab| tab.id == tab_id && tab.pinned),
            shell.tabs.len(),
        ),
        _ => (false, 0),
    });
    rsx! {
        ReviewActionDock {
            unpushed,
            close_disabled: pinned || !viewer.actions_enabled(),
            pinned,
            tab_navigation_disabled: tab_count < 2 || !viewer.actions_enabled(),
            collapsed: collapsed(),
            onclose: move |_| viewer.close_tab(tab_id, true),
            onstep: move |direction| {
                navigation
                    .step
                    .call(ViewerTabStep {
                        direction,
                        focus_tab: false,
                    });
            },
            oncollapsedchange: move |value| collapsed.set(value),
            push: rsx! {
                PushButton {
                    id: "review-push-trigger",
                    source: CreateViewerPush::View {
                        identity: identity(),
                    },
                    disabled: !available,
                    placement: PushButtonPlacement::ReviewDock,
                    title,
                }
            },
        }
    }
}

/// Floats Push and Close between previous and next diff controls at the document's bottom center.
///
/// Collapsing leaves a reveal handle; the push shortcut keeps working while the dock is hidden.
#[component]
pub(crate) fn ReviewActionDock(
    unpushed: Option<bool>,
    close_disabled: bool,
    #[props(default)] pinned: bool,
    tab_navigation_disabled: bool,
    collapsed: bool,
    push: Element,
    onclose: EventHandler<MouseEvent>,
    onstep: EventHandler<ViewerTabDirection>,
    oncollapsedchange: EventHandler<bool>,
) -> Element {
    let language = use_language();
    let navigation_state = if tab_navigation_disabled {
        ButtonState::Disabled
    } else {
        ButtonState::Enabled
    };
    let previous_label = t!(language, "review-previous-diff");
    let next_label = t!(language, "review-next-diff");
    let hide_label = t!(language, "review-actions-hide");
    let show_label = t!(language, "review-actions-show");
    rsx! {
        div {
            class: "review-action-dock",
            "data-collapsed": collapsed.to_string(),
            span { class: "sr-only", role: "status", aria_live: "polite",
                if unpushed == Some(true) {
                    {t!(language, "review-unpushed-hint")}
                }
            }
            div {
                class: "review-action-buttons",
                role: "group",
                aria_label: t!(language, "review-actions-label"),
                aria_hidden: collapsed.then_some("true"),
                "inert": collapsed.then_some(""),
                Button {
                    id: "review-previous-diff",
                    class: "review-step-action",
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    state: navigation_state,
                    aria_label: previous_label.clone(),
                    title: format!("{previous_label} (Ctrl+Shift+Tab)"),
                    onclick: move |_| onstep.call(ViewerTabDirection::Previous),
                    ChevronLeft { size: 16 }
                }
                {push}
                Button {
                    id: "review-close-trigger",
                    class: "review-close-action",
                    size: ButtonSize::Medium,
                    variant: ButtonVariant::Ghost,
                    state: if close_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                    title: if pinned { t!(language, "review-close-pinned") } else { t!(language, "review-close") },
                    aria_label: t!(language, "review-close"),
                    icon: rsx! {
                        X { size: 16 }
                    },
                    onclick: onclose,
                    {t!(language, "review-close-label")}
                }
                Button {
                    id: "review-next-diff",
                    class: "review-step-action",
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    state: navigation_state,
                    aria_label: next_label.clone(),
                    title: format!("{next_label} (Ctrl+Tab)"),
                    onclick: move |_| onstep.call(ViewerTabDirection::Next),
                    ChevronRight { size: 16 }
                }
                span { class: "review-action-divider", aria_hidden: "true" }
                Button {
                    id: "review-actions-hide",
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    aria_label: hide_label.clone(),
                    title: hide_label,
                    onclick: move |_| {
                        oncollapsedchange.call(true);
                        crate::shared::browser::focus_element("review-actions-show".to_owned());
                    },
                    ChevronDown { size: 16 }
                }
            }
            button {
                id: "review-actions-show",
                r#type: "button",
                class: "review-action-reveal",
                aria_label: show_label.clone(),
                aria_hidden: (!collapsed).then_some("true"),
                title: show_label,
                tabindex: if collapsed { "0" } else { "-1" },
                onclick: move |_| {
                    oncollapsedchange.call(false);
                    crate::shared::browser::focus_element("review-actions-hide".to_owned());
                },
                ChevronUp { size: 14 }
            }
        }
    }
}
