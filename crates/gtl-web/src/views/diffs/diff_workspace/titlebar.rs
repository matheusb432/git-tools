use dioxus::prelude::*;
use gtl_models::{
    diffs::ExtensionFilterMode,
    git::{GitHead, GitRevision},
    settings::ViewerLanguage,
};
use gtl_wire::viewer::ViewerAppliedExtensionFilter;
use lucide_dioxus::ChevronsDownUp;

use crate::shared::{
    browser,
    i18n::{t, use_language},
    ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant},
};

#[component]
pub(super) fn ArtifactTitlebar(
    sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    artifact_view_id: Option<String>,
) -> Element {
    let language = use_language();
    let workspace = super::use_workspace_context();
    let view = workspace.view.read();
    rsx! {
        header { class: "diff-workspace-titlebar min-w-0 gap-3 px-3 py-2 mobile:gap-1 mobile:px-2 mobile:py-1.5",
            div { class: "min-w-0 mobile:hidden",
                RepositoryIdentity { repository_name: view.repository_name.clone() }
            }
            if view.modified_files {
                span { class: "text-xs text-acc", {t!(language, "titlebar-working-tree")} }
            } else {
                BranchRange {
                    branch: view.branch.clone(),
                    upstream: view.upstream.clone(),
                }
            }
            if artifact_view_id.is_some() && let Some(applied) = view.extension_filter.clone() {
                div { class: "mobile:hidden",
                    HiddenFilesBadge { applied }
                }
            }
            div { class: "flex-1 mobile:hidden" }
            div { class: "workspace:hidden",
                super::path_filter::PathFilterTrigger { artifact_view_id: artifact_view_id.clone() }
            }
            if artifact_view_id.is_some() {
                super::sidebars::SidebarButtons {
                    visibility: sidebars,
                    keybindings: gtl_models::viewer::ViewerKeybindings::default(),
                    artifact: true,
                }
            }
            CollapseFilesButton { artifact_view_id }
        }
    }
}

#[component]
fn RepositoryIdentity(repository_name: String) -> Element {
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
fn BranchRange(branch: GitHead, upstream: GitRevision) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center gap-1.5 text-ink-2 mobile:flex-1",
            span { class: "truncate text-acc", "{branch}" }
            span { class: "text-ink-3", "\u{2192}" }
            span { class: "truncate text-ink-3", "{upstream}" }
        }
    }
}

/// Discloses the files a saved extension filter hid from an offline artifact.
#[component]
fn HiddenFilesBadge(applied: ViewerAppliedExtensionFilter) -> Element {
    let language = use_language();
    rsx! {
        Badge {
            class: "flex-none cursor-help whitespace-nowrap px-2 py-0.5 text-xs font-semibold",
            variant: BadgeVariant::Deletion,
            title: hidden_files_tooltip(&applied, language),
            {hidden_files_label(&applied, language)}
        }
    }
}

#[component]
pub(super) fn CollapseFilesButton(artifact_view_id: Option<String>) -> Element {
    let language = use_language();
    let workspace = super::use_workspace_context();
    #[cfg(feature = "desktop")]
    let presentation = try_use_context::<crate::views::diffs::presentation::DiffPresentation>();
    let files_folded = (workspace.files_folded)().unwrap_or(false);
    let artifact = artifact_view_id.is_some();
    let fold_label = if files_folded {
        if artifact {
            t!(language, "titlebar-expand-all")
        } else {
            t!(language, "files-expand-diffs")
        }
    } else if artifact {
        t!(language, "titlebar-collapse-all")
    } else {
        t!(language, "files-collapse-diffs")
    };

    rsx! {
        Button {
            class: "mobile:size-11 mobile:p-0",
            size: if artifact { ButtonSize::Small } else { ButtonSize::IconSmall },
            variant: if artifact { ButtonVariant::Outline } else { ButtonVariant::Ghost },
            aria_label: fold_label.clone(),
            title: fold_label.clone(),
            "data-gtl-action": artifact_view_id.as_ref().map(|_| "toggle-files"),
            onclick: move |_| {
                #[cfg(feature = "desktop")]
                if let Some(presentation) = presentation {
                    presentation.toggle_files(workspace.view.peek().identity.tab_id);
                    return;
                }
                let folded = !files_folded;
                let mut folded_state = workspace.files_folded;
                folded_state.set(Some(folded));
                if folded {
                    browser::scroll_diff_document_to_start();
                }
            },
            span {
                class: "inline-flex flex-none mobile:[&_svg]:size-5",
                aria_hidden: "true",
                if files_folded {
                    lucide_dioxus::ChevronsUpDown { size: 14 }
                } else {
                    ChevronsDownUp { size: 14 }
                }
            }
            span {
                class: if artifact { "mobile:hidden" } else { "sr-only" },
                "data-gtl-files-label": artifact_view_id.as_ref().map(|_| ""),
                {fold_label}
            }
        }
    }
}

fn hidden_files_label(applied: &ViewerAppliedExtensionFilter, language: ViewerLanguage) -> String {
    let count = applied.hidden_paths.len();
    let extensions = applied.filter.extensions().extensions().join(", ");
    match applied.filter.mode() {
        ExtensionFilterMode::Hide => t!(
            language,
            "titlebar-hidden-files",
            count = count,
            extensions = extensions
        ),
        ExtensionFilterMode::Only => t!(
            language,
            "titlebar-hidden-files-only",
            count = count,
            extensions = extensions
        ),
    }
}

fn hidden_files_tooltip(
    applied: &ViewerAppliedExtensionFilter,
    language: ViewerLanguage,
) -> String {
    let mut tooltip = t!(language, "titlebar-hidden-tooltip");
    for path in &applied.hidden_paths {
        tooltip.push('\n');
        tooltip.push_str(path.to_string_lossy().as_ref());
    }
    tooltip
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions},
        settings::ViewerLanguage,
    };
    use gtl_wire::viewer::ViewerAppliedExtensionFilter;

    use super::hidden_files_label;
    use crate::test_support::{TestResult, repository_relative_path};

    #[test]
    fn hidden_files_label_names_the_filter_rule() -> TestResult {
        let hidden_paths = vec![
            repository_relative_path("Cargo.lock")?,
            repository_relative_path("web/yarn.lock")?,
        ];
        let hiding = ViewerAppliedExtensionFilter {
            filter: ExtensionFilter::new(ExtensionFilterMode::Hide, FileExtensions::new(["lock"])),
            hidden_paths: hidden_paths.clone(),
        };
        let showing_only = ViewerAppliedExtensionFilter {
            filter: ExtensionFilter::new(
                ExtensionFilterMode::Only,
                FileExtensions::new(["rs", "toml"]),
            ),
            hidden_paths,
        };

        assert_eq!(
            hidden_files_label(&hiding, ViewerLanguage::EnUs),
            "2 files hidden · lock"
        );
        assert_eq!(
            hidden_files_label(&hiding, ViewerLanguage::PtBr),
            "2 arquivos ocultos · lock"
        );
        assert_eq!(
            hidden_files_label(&showing_only, ViewerLanguage::EnUs),
            "2 files hidden · only rs, toml"
        );
        Ok(())
    }
}
