use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerViewIdentity, push::CreateViewerPush};

use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    shared::{
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonState, ButtonVariant},
    },
    views::push::{
        PushButton, PushButtonPlacement, availability::use_view_push_availability,
        use_diff_push_shortcut,
    },
};

#[component]
pub(super) fn ReviewActions(
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
) -> Element {
    let viewer = use_context::<ViewerContext>();
    let (available, title, unpushed) = use_view_push_availability(identity, disabled);
    use_diff_push_shortcut(
        CreateViewerPush::View {
            identity: identity(),
        },
        !available,
    );
    let tab_id = identity().tab_id;
    let pinned = viewer.shell().with(|shell| match shell {
        ViewerShellLoad::Ready(shell) => {
            shell.tabs.iter().any(|tab| tab.id == tab_id && tab.pinned)
        }
        _ => false,
    });
    rsx! {
        ReviewActionDock {
            unpushed,
            close_disabled: pinned || !viewer.actions_enabled(),
            pinned,
            onclose: move |_| viewer.close_tab(tab_id, true),
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

#[component]
pub(crate) fn ReviewActionDock(
    unpushed: Option<bool>,
    close_disabled: bool,
    #[props(default)] pinned: bool,
    push: Element,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "review-action-dock",
            role: "group",
            aria_label: t!(language, "review-actions-label"),
            span { class: "sr-only", role: "status", aria_live: "polite",
                if unpushed == Some(true) {
                    {t!(language, "review-unpushed-hint")}
                }
            }
            div {
                class: "review-action-buttons",
                "data-unpushed": (unpushed == Some(true)).to_string(),
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
                        lucide_dioxus::X { size: 16 }
                    },
                    onclick: onclose,
                    {t!(language, "review-close-label")}
                }
            }
        }
    }
}
