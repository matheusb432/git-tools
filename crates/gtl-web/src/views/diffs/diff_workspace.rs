use dioxus::prelude::*;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::ViewerActiveView;
#[cfg(feature = "artifact")]
use lucide_dioxus::{History, Menu, SlidersHorizontal};

#[cfg(feature = "artifact")]
use self::titlebar::{ViewActions, ViewActionsLayout};
use self::{
    commits_panel::CommitsPanel, files_panel::FilesPanel, keybar::Keybar, titlebar::ViewTitlebar,
};
#[cfg(feature = "artifact")]
use crate::shared::{
    browser,
    ui::{
        Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, CountBadge, CountBadgeSize,
        Popover,
    },
};
use crate::{entities::diffs::ClientDiffSource, views::diffs::ClientDiffDocument};

mod commits_panel;
#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
mod display_controls;
mod files_panel;
mod keybar;
mod titlebar;

#[cfg(feature = "desktop")]
pub(crate) use desktop::DiffWorkspaceView;

#[cfg(feature = "artifact")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactMobilePanel {
    Files,
    Commits,
    View,
}

#[cfg(feature = "artifact")]
#[component]
pub(crate) fn ArtifactDiffWorkspace(view: ViewerActiveView) -> Element {
    let mut mobile_panel = use_signal(|| None::<ArtifactMobilePanel>);
    let mut file_filter = use_signal(String::new);
    let mut files_folded = use_signal(|| None::<bool>);
    let mut copy_context_enabled = use_signal(|| true);
    let mut flashing_file = use_signal(|| None::<String>);
    let tab_id = view.identity.tab_id;
    let files_trigger = format!("artifact-files-trigger-{tab_id}");
    let commits_trigger = format!("artifact-commits-trigger-{tab_id}");
    let view_trigger = format!("artifact-view-trigger-{tab_id}");

    let onnavigate = move |anchor_id: String| {
        mobile_panel.set(None);
        browser::scroll_to_file(&anchor_id);
        flashing_file.set(Some(anchor_id.clone()));
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(1_200)).await;
            if flashing_file().as_deref() == Some(anchor_id.as_str()) {
                flashing_file.set(None);
            }
        });
    };
    let mobile_navigation = rsx! {
        ArtifactNavigationButton {
            id: files_trigger.clone(),
            label: "Files",
            aria_label: "Changed files",
            panel: ArtifactMobilePanel::Files,
            count: view.files.len(),
            enabled: !view.files.is_empty(),
            onclick: move |_| mobile_panel.set(Some(ArtifactMobilePanel::Files)),
        }
        ArtifactNavigationButton {
            id: commits_trigger.clone(),
            label: "History",
            panel: ArtifactMobilePanel::Commits,
            count: view.commits.len(),
            enabled: !view.commits.is_empty(),
            onclick: move |_| mobile_panel.set(Some(ArtifactMobilePanel::Commits)),
        }
        ArtifactNavigationButton {
            id: view_trigger.clone(),
            label: "View",
            aria_label: "View settings",
            panel: ArtifactMobilePanel::View,
            enabled: true,
            onclick: move |_| mobile_panel.set(Some(ArtifactMobilePanel::View)),
        }
    };

    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                source: ClientDiffSource::Artifact,
                view: view.clone(),
                files_folded: files_folded(),
                copy_context_enabled: copy_context_enabled(),
                file_filter: file_filter(),
                flashing_file: flashing_file(),
                onfold: move |folded| files_folded.set(Some(folded)),
                oncontext: move |enabled| copy_context_enabled.set(enabled),
                onfilter: move |value| file_filter.set(value),
                onnavigate,
                mobile_navigation,
            }
        }

        Popover {
            id: format!("artifact-files-panel-{tab_id}"),
            trigger_id: files_trigger,
            open: mobile_panel() == Some(ArtifactMobilePanel::Files),
            title: "Changed files",
            onclose: move |()| mobile_panel.set(None),
            FilesPanel {
                view: view.clone(),
                filter: file_filter(),
                onfilter: move |value| file_filter.set(value),
                onnavigate,
            }
        }
        Popover {
            id: format!("artifact-commits-panel-{tab_id}"),
            trigger_id: commits_trigger,
            open: mobile_panel() == Some(ArtifactMobilePanel::Commits),
            title: "Commits",
            onclose: move |()| mobile_panel.set(None),
            CommitsPanel { view: view.clone() }
        }
        Popover {
            id: format!("artifact-view-panel-{tab_id}"),
            trigger_id: view_trigger,
            open: mobile_panel() == Some(ArtifactMobilePanel::View),
            title: "View settings",
            onclose: move |()| mobile_panel.set(None),
            ViewActions {
                layout: ViewActionsLayout::Panel,
                files_folded: files_folded().unwrap_or(false),
                copy_context_enabled: copy_context_enabled(),
                onfold: move |folded| files_folded.set(Some(folded)),
                oncontext: move |enabled| copy_context_enabled.set(enabled),
            }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
fn ArtifactNavigationButton(
    id: String,
    label: String,
    aria_label: Option<String>,
    panel: ArtifactMobilePanel,
    count: Option<usize>,
    enabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let aria_label_display = aria_label.unwrap_or_else(|| label.clone());
    rsx! {
        Button {
            id,
            class: "relative hidden min-w-0 flex-col justify-center gap-0.5 px-1 py-1 text- leading-none focus-visible:-outline-offset-2 disabled:cursor-default disabled:opacity-35 mobile:flex",
            layout: ButtonLayout::Content,
            size: ButtonSize::Content,
            variant: ButtonVariant::Ghost,
            state: if enabled { ButtonState::Enabled } else { ButtonState::Disabled },
            aria_label: aria_label_display,
            onclick,
            ArtifactNavigationIcon { panel }
            ArtifactNavigationLabel { label }
            if let Some(count) = count {
                CountBadge {
                    class: "absolute top-1 right-1",
                    count,
                    size: CountBadgeSize::Compact,
                }
            }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
fn ArtifactNavigationIcon(panel: ArtifactMobilePanel) -> Element {
    rsx! {
        span { class: "[&_svg]:size-5", aria_hidden: "true",
            match panel {
                ArtifactMobilePanel::Files => rsx! {
                    Menu { size: 20 }
                },
                ArtifactMobilePanel::Commits => rsx! {
                    History { size: 20 }
                },
                ArtifactMobilePanel::View => rsx! {
                    SlidersHorizontal { size: 20 }
                },
            }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
fn ArtifactNavigationLabel(label: String) -> Element {
    rsx! {
        span { "{label}" }
    }
}

#[component]
fn DiffWorkspaceDocument(
    source: ClientDiffSource,
    view: ViewerActiveView,
    files_folded: Option<bool>,
    copy_context_enabled: bool,
    file_filter: String,
    flashing_file: Option<String>,
    onfold: EventHandler<bool>,
    oncontext: EventHandler<bool>,
    onfilter: EventHandler<String>,
    onnavigate: EventHandler<String>,
    mobile_navigation: Option<Element>,
    onselect_commit: Option<EventHandler<String>>,
    onclear_commit: Option<EventHandler<()>>,
    onopen: Option<EventHandler<String>>,
) -> Element {
    let footer = view.footer.clone();

    rsx! {
        div { class: "grid h-full min-h-0 grid-cols-[0_minmax(0,1fr)_0] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden workspace:grid-cols-[220px_minmax(0,1fr)_210px] expanded:grid-cols-[262px_minmax(0,1fr)_252px] wide-screen:grid-cols-[320px_minmax(0,1fr)_304px]",
            ViewTitlebar {
                view: view.clone(),
                files_folded: files_folded.unwrap_or(false),
                copy_context_enabled,
                mobile_navigation,
                onfold,
                oncontext,
            }
            aside {
                class: "col-start-1 row-start-2 hidden min-h-0 overflow-hidden border-r border-line bg-surface workspace:block",
                aria_label: "Changed files",
                FilesPanel {
                    view: view.clone(),
                    filter: file_filter,
                    test_id: Some(test_ids::CHANGED_FILES_PANEL.value().to_owned()),
                    onfilter,
                    onnavigate,
                }
            }
            ClientDiffDocument {
                source,
                view: view.clone(),
                folded: files_folded,
                copy_context_enabled,
                flashing_file,
                onopen,
            }
            aside {
                class: "col-start-3 row-start-2 hidden min-h-0 overflow-hidden border-l border-line bg-surface workspace:block",
                aria_label: "Commits",
                CommitsPanel {
                    view,
                    onselect: onselect_commit,
                    onclear: onclear_commit,
                }
            }
            Keybar { footer }
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobilePanel {
    Display,
    Files,
    Commits,
}

// TODO: refactor these to cleaner, intl compatible shape
const fn file_label(count: usize) -> &'static str {
    if count == 1 { "file" } else { "files" }
}

const fn commit_label(count: usize) -> &'static str {
    if count == 1 { "commit" } else { "commits" }
}
