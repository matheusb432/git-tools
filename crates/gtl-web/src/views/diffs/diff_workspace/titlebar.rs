use dioxus::prelude::*;
use gtl_models::git::{GitHead, GitRevision};
use gtl_wire::viewer::ViewerAppliedExclusions;

use crate::shared::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewActionsLayout {
    Panel,
    Toolbar,
}

#[component]
pub(super) fn ViewTitlebar(
    mobile_navigation: Option<Element>,
    artifact_view_id: Option<String>,
) -> Element {
    let workspace = super::use_workspace_context();
    let view = workspace.view.read();
    rsx! {
        header { class: "col-span-3 row-start-1 flex min-w-0 items-center gap-4 border-b border-line bg-surface px-5 py-3 tablet:flex-wrap tablet:gap-2.5 tablet:px-3 tablet:py-2.5 mobile:gap-1.5 mobile:px-2 mobile:py-2",
            RepositoryIdentity {
                repository_name: view.repository_name.clone(),
                title: view.title.clone(),
            }
            BranchRange { branch: view.branch.clone(), upstream: view.upstream.clone() }
            if let Some(exclusions) = &view.exclusions {
                ExclusionsBadge { exclusions: exclusions.clone() }
            }
            div { class: "flex-1" }
            if let Some(mobile_navigation) = mobile_navigation {
                {mobile_navigation}
            }
            ViewActions { layout: ViewActionsLayout::Toolbar, artifact_view_id }
        }
    }
}

#[component]
fn RepositoryIdentity(repository_name: String, title: String) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-baseline gap-2 text-lg font-semibold tracking-tight mobile:text-base",
            span { class: "truncate",
                "~/"
                b { class: "font-bold text-acc", "{repository_name}" }
            }
            Badge {
                class: "flex-none self-center px-2 py-0.5 text-xs",
                variant: BadgeVariant::Accent,
                "{title}"
            }
        }
    }
}

#[component]
fn BranchRange(branch: GitHead, upstream: GitRevision) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center gap-1.5 text-ink-2 tablet:order-3 tablet:w-full",
            span { class: "truncate text-acc", "{branch}" }
            span { class: "text-ink-3", "\u{2192}" }
            span { class: "truncate text-ink-3", "{upstream}" }
        }
    }
}

#[component]
fn ExclusionsBadge(exclusions: ViewerAppliedExclusions) -> Element {
    rsx! {
        Badge {
            class: "flex-none cursor-help whitespace-nowrap px-2 py-0.5 text-xs font-semibold",
            variant: BadgeVariant::Deletion,
            title: exclusion_tooltip(&exclusions),
            {exclusion_label(&exclusions)}
        }
    }
}

#[component]
pub(super) fn ViewActions(layout: ViewActionsLayout, artifact_view_id: Option<String>) -> Element {
    let mut workspace = super::use_workspace_context();
    let files_folded = (workspace.files_folded)().unwrap_or(false);
    let copy_context_enabled = (workspace.copy_context_enabled)();
    let container_classes = match layout {
        ViewActionsLayout::Panel => "grid grid-cols-2 gap-2",
        ViewActionsLayout::Toolbar => "flex items-center gap-2 mobile:hidden",
    };
    let button_size = match layout {
        ViewActionsLayout::Panel => ButtonSize::Medium,
        ViewActionsLayout::Toolbar => ButtonSize::Small,
    };
    let artifact_selected_classes = artifact_view_id
        .as_ref()
        .map(|_| ButtonVariant::Pressed.classes());
    let artifact_unselected_classes = artifact_view_id
        .as_ref()
        .map(|_| ButtonVariant::Outline.classes());

    rsx! {
        div { class: "{container_classes}",
            Button {
                size: button_size,
                variant: ButtonVariant::Outline,
                title: "Collapse or expand all files",
                "data-gtl-action": artifact_view_id.as_ref().map(|_| "toggle-files"),
                onclick: move |_| workspace.files_folded.set(Some(!files_folded)),
                if files_folded {
                    "Expand all"
                } else {
                    "Collapse all"
                }
            }
            Button {
                size: button_size,
                variant: if copy_context_enabled { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                aria_pressed: copy_context_enabled.to_string(),
                title: "Prepend a commented path and selected line range when copying diff lines",
                "data-gtl-action": artifact_view_id.as_ref().map(|_| "toggle-copy-context"),
                "data-gtl-selected-classes": artifact_selected_classes,
                "data-gtl-unselected-classes": artifact_unselected_classes,
                onclick: move |_| workspace.copy_context_enabled.set(!copy_context_enabled),
                "+ context"
            }
        }
    }
}

fn exclusion_label(exclusions: &ViewerAppliedExclusions) -> String {
    let hidden_count = exclusions.hidden_paths.len();
    let extension_label = if exclusions.extensions.is_empty() {
        "configured".to_owned()
    } else {
        exclusions.extensions.extensions().join(", ")
    };
    let file_label = super::file_label(hidden_count);
    format!("{hidden_count} {file_label} hidden · {extension_label}")
}

fn exclusion_tooltip(exclusions: &ViewerAppliedExclusions) -> String {
    let mut tooltip = String::from("Hidden by git-tools config diff.exclude:");
    for path in &exclusions.hidden_paths {
        tooltip.push('\n');
        tooltip.push_str(path.to_string_lossy().as_ref());
    }
    tooltip
}
