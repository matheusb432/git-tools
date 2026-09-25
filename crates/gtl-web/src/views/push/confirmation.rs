use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::push::{ViewerPushCommandArgument, ViewerPushPreview};
use lucide_dioxus::{Check, CircleQuestionMark, Copy, X};

use crate::shared::{
    browser,
    i18n::{t, use_language},
    ui::{
        Button, ButtonSize, ButtonVariant, HoverPopover, HoverPopoverPlacement, use_hover_popover,
    },
};

#[component]
pub(crate) fn PushConfirmationDetails(preview: ViewerPushPreview) -> Element {
    let language = use_language();
    let mut copy_result = use_signal(|| None::<bool>);
    let command_to_copy = preview.command.clone();
    let copy_label = match copy_result() {
        Some(true) => t!(language, "push-command-copied"),
        Some(false) => t!(language, "push-command-copy-failed"),
        None => t!(language, "push-command-copy"),
    };
    rsx! {
        dl { class: "push-confirmation-data",
            if let Some(project) = &preview.project {
                dt { {t!(language, "push-detail-project")} }
                dd { "{project}" }
            } else {
                dt { {t!(language, "push-detail-directory")} }
                dd { "{preview.repository}" }
            }
            dt { {t!(language, "push-detail-branch")} }
            dd { "{preview.branch}" }
            if preview.branch != preview.remote_branch {
                dt { {t!(language, "push-detail-remote-branch")} }
                dd { "{preview.remote_branch}" }
            }
            dt { {t!(language, "push-detail-commits")} }
            dd { "{preview.count}" }
            dt { {t!(language, "push-detail-remote")} }
            dd {
                span { "{preview.remote}" }
                span { class: "text-ink-2", " ({preview.remote_url})" }
            }
            dt { {t!(language, "push-detail-selected-sha")} }
            dd {
                code { "{preview.commit}" }
            }
        }
        div { class: "push-confirmation-command-label",
            span { {t!(language, "push-detail-command")} }
            PushCommandHelp { arguments: preview.command_arguments.clone() }
        }
        div { class: "relative",
            Button {
                class: "absolute top-1 left-1 z-10",
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                aria_label: copy_label.clone(),
                title: copy_label.clone(),
                onclick: move |_| {
                    let command = command_to_copy.clone();
                    spawn(async move {
                        copy_result.set(Some(browser::copy_text(&command).await));
                    });
                },
                span { aria_hidden: "true",
                    match copy_result() {
                        Some(true) => rsx! {
                            Check { size: 16 }
                        },
                        Some(false) => rsx! {
                            X { size: 16 }
                        },
                        None => rsx! {
                            Copy { size: 16 }
                        },
                    }
                }
            }
            span { class: "sr-only", role: "status", aria_live: "polite",
                if copy_result().is_some() {
                    "{copy_label}"
                }
            }
            pre { class: "push-confirmation-command",
                code { "{preview.command}" }
            }
        }
    }
}

#[component]
fn PushCommandHelp(arguments: Vec<ViewerPushCommandArgument>) -> Element {
    let language = use_language();
    let id = "viewer-push-command-help".to_owned();
    let anchor_name = "--viewer-push-command-help".to_owned();
    let hover = use_hover_popover(id.clone());
    rsx! {
        span {
            class: "inline-flex",
            style: "anchor-name: {anchor_name};",
            onmouseenter: move |_| hover.pointer_enter.call(()),
            onmouseleave: move |_| hover.pointer_leave.call(()),
            onfocusin: move |_| hover.focus_enter.call(()),
            onfocusout: move |_| hover.focus_leave.call(()),
            Button {
                size: ButtonSize::IconCompact,
                variant: ButtonVariant::Ghost,
                aria_label: t!(language, "push-command-help"),
                aria_describedby: id.clone(),
                popovertarget: id.clone(),
                popovertargetaction: "toggle",
                span { aria_hidden: "true",
                    CircleQuestionMark { size: 15 }
                }
            }
            HoverPopover {
                id,
                anchor_name: anchor_name.clone(),
                aria_label: t!(language, "push-command-help"),
                placement: HoverPopoverPlacement::Below,
                ul { class: "grid gap-2 text-xs",
                    for argument in arguments {
                        li { key: "{argument:?}", class: "grid gap-0.5",
                            code { class: "font-semibold text-ink", {argument_label(argument)} }
                            if let Some(description) = argument_description(argument, language) {
                                span { class: "text-ink-2", {description} }
                            }
                        }
                    }
                }
            }
        }
    }
}

const fn argument_label(argument: ViewerPushCommandArgument) -> &'static str {
    match argument {
        ViewerPushCommandArgument::Git => "git",
        ViewerPushCommandArgument::WorkingDirectory => "-C <directory>",
        ViewerPushCommandArgument::DisableMirroring => "-c remote.<remote>.mirror=false",
        ViewerPushCommandArgument::Push => "push",
        ViewerPushCommandArgument::Atomic => "--atomic",
        ViewerPushCommandArgument::Porcelain => "--porcelain",
        ViewerPushCommandArgument::NoFollowTags => "--no-follow-tags",
        ViewerPushCommandArgument::NoRecurseSubmodules => "--recurse-submodules=no",
        ViewerPushCommandArgument::OptionSeparator => "--",
        ViewerPushCommandArgument::Remote => "<remote>",
        ViewerPushCommandArgument::CommitRef => "<SHA>:refs/heads/<branch>",
    }
}

fn argument_description(
    argument: ViewerPushCommandArgument,
    language: ViewerLanguage,
) -> Option<String> {
    match argument {
        ViewerPushCommandArgument::Git => None,
        ViewerPushCommandArgument::WorkingDirectory => {
            Some(t!(language, "push-command-arg-directory"))
        }
        ViewerPushCommandArgument::DisableMirroring => {
            Some(t!(language, "push-command-arg-mirror"))
        }
        ViewerPushCommandArgument::Push => Some(t!(language, "push-command-arg-push")),
        ViewerPushCommandArgument::Atomic => Some(t!(language, "push-command-arg-atomic")),
        ViewerPushCommandArgument::Porcelain => Some(t!(language, "push-command-arg-porcelain")),
        ViewerPushCommandArgument::NoFollowTags => Some(t!(language, "push-command-arg-tags")),
        ViewerPushCommandArgument::NoRecurseSubmodules => {
            Some(t!(language, "push-command-arg-submodules"))
        }
        ViewerPushCommandArgument::OptionSeparator => {
            Some(t!(language, "push-command-arg-separator"))
        }
        ViewerPushCommandArgument::Remote => Some(t!(language, "push-command-arg-remote")),
        ViewerPushCommandArgument::CommitRef => Some(t!(language, "push-command-arg-ref")),
    }
}
