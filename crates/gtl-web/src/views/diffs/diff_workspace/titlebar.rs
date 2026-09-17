use dioxus::prelude::*;
use gtl_models::git::{GitHead, GitRevision};
use gtl_wire::viewer::ViewerAppliedExclusions;
use lucide_dioxus::ChevronsDownUp;
#[cfg(feature = "component-preview")]
use lucide_dioxus::Search;

use crate::shared::{
    browser,
    ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant},
};

#[component]
pub(super) fn ViewTitlebar(
    sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    keybindings: gtl_models::viewer::ViewerKeybindings,
    ontoggle_sidebar: Option<EventHandler<super::sidebars::Sidebar>>,
    live_actions: Option<Element>,
    artifact_view_id: Option<String>,
) -> Element {
    let workspace = super::use_workspace_context();
    let view = workspace.view.read();
    rsx! {
        header { class: "diff-workspace-titlebar min-w-0 gap-4 px-5 py-3 tablet:gap-2.5 tablet:px-3 tablet:py-2.5 mobile:gap-1 mobile:px-2 mobile:py-1.5",
            div { class: "min-w-0 mobile:hidden",
                RepositoryIdentity { repository_name: view.repository_name.clone() }
            }
            if view.modified_files {
                span { class: "text-xs text-acc", "Working tree · HEAD" }
            } else {
                BranchRange {
                    branch: view.branch.clone(),
                    upstream: view.upstream.clone(),
                }
            }
            ExtensionsControl {
                artifact: artifact_view_id.is_some(),
                exclusions: view.exclusions.clone(),
            }
            div { class: "flex-1 mobile:hidden" }
            if let Some(live_actions) = live_actions {
                {live_actions}
            }
            div { class: "workspace:hidden",
                super::path_filter::PathFilterTrigger { artifact_view_id: artifact_view_id.clone() }
            }
            super::sidebars::SidebarButtons {
                visibility: sidebars,
                keybindings,
                ontoggle: ontoggle_sidebar,
                artifact: artifact_view_id.is_some(),
            }
            CollapseFilesButton { artifact_view_id }
        }
    }
}

#[cfg(feature = "component-preview")]
#[component]
pub(super) fn PreviewViewTitlebar(
    #[props(default)] mobile: bool,
    onfindall: EventHandler<()>,
) -> Element {
    let workspace = super::use_workspace_context();
    let view = workspace.view.read();
    let header_classes = if mobile {
        "col-span-3 row-start-1 flex min-w-0 items-center gap-1 border-b border-line bg-surface px-2 py-1.5"
    } else {
        "col-span-3 row-start-1 flex min-w-0 items-center gap-4 border-b border-line bg-surface px-5 py-3"
    };

    rsx! {
        header { class: "diff-workspace-titlebar min-w-0 {header_classes}",
            if !mobile {
                RepositoryIdentity { repository_name: view.repository_name.clone() }
            }
            BranchRange {
                branch: view.branch.clone(),
                upstream: view.upstream.clone(),
                compact: mobile,
            }
            if !mobile {
                if let Some(exclusions) = &view.exclusions {
                    ExclusionsBadge { exclusions: exclusions.clone() }
                }
            }
            if !mobile {
                div { class: "flex-1" }
                FindAllFilesButton { onfindall }
            }
            if mobile {
                super::path_filter::PathFilterTrigger {}
            }
            CollapseFilesButton { preview_mobile: mobile }
        }
    }
}

#[cfg(feature = "component-preview")]
#[component]
fn FindAllFilesButton(onfindall: EventHandler<()>) -> Element {
    rsx! {
        Button {
            size: ButtonSize::Small,
            variant: ButtonVariant::Outline,
            aria_label: "Search code in all files",
            title: "Search code in all files",
            onclick: move |_| onfindall.call(()),
            span { class: "inline-flex flex-none", aria_hidden: "true",
                Search { size: 15 }
            }
            span { "All files" }
        }
    }
}

#[component]
pub(super) fn RepositoryIdentity(repository_name: String) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-baseline gap-2 text-lg font-semibold tracking-tight mobile:text-base",
            span { class: "truncate",
                "~/"
                b { class: "font-bold text-acc", "{repository_name}" }
            }
        }
    }
}

#[component]
pub(super) fn BranchRange(
    branch: GitHead,
    upstream: GitRevision,
    #[props(default)] compact: bool,
) -> Element {
    let classes = if compact {
        "flex min-w-0 flex-1 items-center gap-1.5 text-ink-2"
    } else {
        "flex min-w-0 items-center gap-1.5 text-ink-2 mobile:flex-1"
    };
    rsx! {
        div { class: "{classes}",
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
fn CollapseFilesButton(
    artifact_view_id: Option<String>,
    #[props(default)] preview_mobile: bool,
) -> Element {
    let mut workspace = super::use_workspace_context();
    let files_folded = (workspace.files_folded)().unwrap_or(false);
    let button_size = if preview_mobile {
        ButtonSize::IconTouch
    } else {
        ButtonSize::Small
    };
    let icon_size = if preview_mobile { 18 } else { 14 };

    rsx! {
        Button {
            class: "mobile:size-11 mobile:p-0",
            size: button_size,
            variant: ButtonVariant::Outline,
            aria_label: if files_folded { "Expand all" } else { "Collapse all" },
            title: if files_folded { "Expand all" } else { "Collapse all" },
            "data-gtl-action": artifact_view_id.as_ref().map(|_| "toggle-files"),
            onclick: move |_| {
                let folded = !files_folded;
                workspace.files_folded.set(Some(folded));
                #[cfg(feature = "desktop")]
                workspace
                    .fold_command
                    .set(
                        Some(super::FileFoldCommand {
                            tab_id: workspace.view.peek().identity.tab_id,
                            folded,
                        }),
                    );
                if folded {
                    browser::scroll_diff_document_to_start();
                }
            },
            span {
                class: "inline-flex flex-none mobile:[&_svg]:size-5",
                aria_hidden: "true",
                ChevronsDownUp { size: icon_size }
            }
            span {
                class: if preview_mobile { "hidden" } else { "mobile:hidden" },
                "data-gtl-files-label": artifact_view_id.as_ref().map(|_| ""),
                if files_folded {
                    "Expand all"
                } else {
                    "Collapse all"
                }
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

#[component]
fn ExtensionsControl(artifact: bool, exclusions: Option<ViewerAppliedExclusions>) -> Element {
    #[cfg(feature = "desktop")]
    if !artifact {
        return rsx! {
            super::extension_filters::desktop::ExtensionFilters {}
        };
    }
    let _ = artifact;
    rsx! {
        if let Some(exclusions) = exclusions {
            div { class: "mobile:hidden",
                ExclusionsBadge { exclusions }
            }
        }
    }
}
