use dioxus::prelude::*;
use gtl_models::diffs::CommitId;
#[cfg(feature = "artifact")]
use gtl_models::viewer::ViewerTabId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::ViewerActiveView;
#[cfg(feature = "artifact")]
use gtl_wire::viewer::ViewerDiffFileId;
#[cfg(feature = "artifact")]
use lucide_dioxus::{History, Menu, SlidersHorizontal};

#[cfg(feature = "artifact")]
use self::titlebar::{ViewActions, ViewActionsLayout};
use self::{
    commits_panel::CommitsPanel, files_panel::FilesPanel, keybar::Keybar, titlebar::ViewTitlebar,
};
#[cfg(feature = "artifact")]
use crate::shared::ui::{
    Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, CountBadge, CountBadgeSize,
    Popover,
};
#[cfg(feature = "artifact")]
use crate::{entities::diffs::ClientDiffWorkspace, views::diffs::StaticDiffDocument};

pub(crate) mod commits_panel;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArtifactViewMarkup {
    tab_id: ViewerTabId,
}

#[cfg(feature = "artifact")]
impl ArtifactViewMarkup {
    fn new(tab_id: ViewerTabId) -> Self {
        Self { tab_id }
    }

    fn view_id(self) -> String {
        self.tab_id.to_string()
    }

    fn target_prefix(self) -> String {
        format!("artifact-view-{}", self.tab_id)
    }

    fn control_id(self, control: &str) -> String {
        format!("{}-{control}", self.target_prefix())
    }

    fn file_target_id(self, file_id: &ViewerDiffFileId) -> String {
        format!("{}-{}", self.target_prefix(), file_id.as_str())
    }
}

#[cfg(feature = "artifact")]
#[component]
pub(crate) fn ArtifactDiffWorkspace(
    mut view: ViewerActiveView,
    mut workspace: ClientDiffWorkspace,
) -> Element {
    let markup = ArtifactViewMarkup::new(view.identity.tab_id);
    for file in &mut view.files {
        file.anchor_id = markup.file_target_id(&file.id);
    }
    for file in &mut workspace.files {
        file.summary.anchor_id = markup.file_target_id(&file.summary.id);
    }

    let files_trigger = markup.control_id("files-trigger");
    let files_dialog = markup.control_id("files-dialog");
    let commits_trigger = markup.control_id("commits-trigger");
    let commits_dialog = markup.control_id("commits-dialog");
    let view_trigger = markup.control_id("view-trigger");
    let view_dialog = markup.control_id("view-dialog");
    let mobile_navigation = rsx! {
        ArtifactNavigationButton {
            id: files_trigger.clone(),
            dialog_id: files_dialog.clone(),
            label: "Files",
            aria_label: "Changed files",
            panel: ArtifactMobilePanel::Files,
            count: view.files.len(),
            enabled: !view.files.is_empty(),
            onclick: move |_| {},
        }
        ArtifactNavigationButton {
            id: commits_trigger.clone(),
            dialog_id: commits_dialog.clone(),
            label: "History",
            panel: ArtifactMobilePanel::Commits,
            count: view.commits.len(),
            enabled: !view.commits.is_empty(),
            onclick: move |_| {},
        }
        ArtifactNavigationButton {
            id: view_trigger.clone(),
            dialog_id: view_dialog.clone(),
            label: "View",
            aria_label: "View settings",
            panel: ArtifactMobilePanel::View,
            enabled: true,
            onclick: move |_| {},
        }
    };
    let diff_document = rsx! {
        StaticDiffDocument { view: view.clone(), workspace }
    };

    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                view: view.clone(),
                diff_document,
                files_folded: None,
                copy_context_enabled: true,
                file_filter: String::new(),
                onfold: move |_| {},
                oncontext: move |_| {},
                onfilter: move |_| {},
                onnavigate: move |_| {},
                mobile_navigation,
                artifact_view_id: Some(markup.view_id()),
            }
        }

        Popover {
            id: files_dialog,
            trigger_id: files_trigger,
            open: false,
            title: "Changed files",
            onclose: move |()| {},
            artifact_view_id: Some(markup.view_id()),
            FilesPanel {
                view: view.clone(),
                filter: String::new(),
                onfilter: move |_| {},
                onnavigate: move |_| {},
                artifact_view_id: Some(markup.view_id()),
            }
        }
        Popover {
            id: commits_dialog,
            trigger_id: commits_trigger,
            open: false,
            title: "Commits",
            onclose: move |()| {},
            artifact_view_id: Some(markup.view_id()),
            CommitsPanel { view: view.clone() }
        }
        Popover {
            id: view_dialog,
            trigger_id: view_trigger,
            open: false,
            title: "View settings",
            onclose: move |()| {},
            artifact_view_id: Some(markup.view_id()),
            ViewActions {
                layout: ViewActionsLayout::Panel,
                files_folded: false,
                copy_context_enabled: true,
                onfold: move |_| {},
                oncontext: move |_| {},
                artifact_view_id: Some(markup.view_id()),
            }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
fn ArtifactNavigationButton(
    id: String,
    dialog_id: String,
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
            aria_controls: dialog_id,
            aria_expanded: "false",
            aria_haspopup: "dialog",
            "data-gtl-action": "open-dialog",
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
    view: ViewerActiveView,
    diff_document: Element,
    files_folded: Option<bool>,
    copy_context_enabled: bool,
    file_filter: String,
    onfold: EventHandler<bool>,
    oncontext: EventHandler<bool>,
    onfilter: EventHandler<String>,
    onnavigate: EventHandler<String>,
    mobile_navigation: Option<Element>,
    onselect_commit: Option<EventHandler<CommitId>>,
    onclear_commit: Option<EventHandler<()>>,
    artifact_view_id: Option<String>,
) -> Element {
    let footer = view.footer.clone();
    let artifact_workspace = artifact_view_id.as_ref().map(|_| "");
    let artifact_files_folded = artifact_view_id
        .as_ref()
        .map(|_| files_folded.unwrap_or(false).to_string());
    let artifact_copy_context = artifact_view_id
        .as_ref()
        .map(|_| copy_context_enabled.to_string());

    rsx! {
        div {
            class: "grid h-full min-h-0 grid-cols-[0_minmax(0,1fr)_0] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden workspace:grid-cols-[220px_minmax(0,1fr)_210px] expanded:grid-cols-[262px_minmax(0,1fr)_252px] wide-screen:grid-cols-[320px_minmax(0,1fr)_304px]",
            "data-gtl-workspace": artifact_workspace,
            "data-gtl-view": artifact_view_id.clone(),
            "data-gtl-files-folded": artifact_files_folded,
            "data-gtl-copy-context": artifact_copy_context,
            ViewTitlebar {
                view: view.clone(),
                files_folded: files_folded.unwrap_or(false),
                copy_context_enabled,
                mobile_navigation,
                onfold,
                oncontext,
                artifact_view_id: artifact_view_id.clone(),
            }
            aside {
                class: "col-start-1 row-start-2 hidden min-h-0 overflow-hidden border-r border-line bg-surface workspace:block",
                aria_label: "Changed files",
                FilesPanel {
                    // TODO: optimize. this (and ClientDiffDocument, and CommitsPanel) clone the view many times, despite most not needing all its data.
                    view: view.clone(),
                    filter: file_filter,
                    test_id: Some(test_ids::CHANGED_FILES_PANEL.value().to_owned()),
                    onfilter,
                    onnavigate,
                    artifact_view_id: artifact_view_id.clone(),
                }
            }
            {diff_document}
            aside {
                class: "col-start-3 row-start-2 hidden min-h-0 overflow-hidden border-l border-line bg-surface workspace:block",
                aria_label: "Commits",
                CommitsPanel {
                    view,
                    test_id: Some(test_ids::COMMITS_PANEL.value().to_owned()),
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

#[cfg(all(test, feature = "artifact"))]
mod artifact_tests {
    use dioxus::prelude::*;
    use gtl_models::{
        diffs::{CommitId, DiffLineCount},
        git::{BranchName, GitHead, GitRevision},
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
    };
    use gtl_wire::viewer::{
        ViewerActiveView, ViewerCommandLine, ViewerCommitSelection, ViewerCommitSummary,
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary,
        ViewerFooter, ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::{ArtifactDiffWorkspace, ArtifactViewMarkup};
    use crate::{
        entities::diffs::{ClientDiffWorkspace, static_diff_workspace},
        test_support::{
            TestResult, absolute_file_path, machine_timestamp, project_name,
            repository_relative_path, viewer_tab_id,
        },
    };

    fn artifact_view(tab_id: u64) -> TestResult<(ViewerActiveView, ClientDiffWorkspace)> {
        let identity = ViewerViewIdentity {
            tab_id: viewer_tab_id(tab_id)?,
            range_generation: ViewerRangeGeneration::new(3),
            selection_generation: ViewerSelectionGeneration::new(5),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        };
        let file = ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/<unsafe>.rs")?,
            absolute_path: absolute_file_path("/repo/src/<unsafe>.rs")?,
            anchor_id: "file-0".to_owned(),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::new(1),
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
        };
        let workspace = static_diff_workspace(identity, vec![(file.clone(), Vec::new())]);
        let view = ViewerActiveView {
            identity,
            title: "diff".to_owned(),
            repository_name: project_name(&format!("repo-{tab_id}"))?,
            branch: GitHead::Branch(BranchName::main()),
            upstream: GitRevision::main(),
            command: ViewerCommandLine {
                lead: "git diff ".to_owned(),
                range: "HEAD~1..HEAD".to_owned(),
                trail: String::new(),
            },
            files: vec![file],
            commits_label: "1 commit".to_owned(),
            commits: vec![ViewerCommitSummary {
                id: CommitId::try_from("0123456789abcdef0123456789abcdef01234567")?,
                subject: "static render".to_owned(),
                body: String::new(),
                committed_at: machine_timestamp("2026-08-19T10:00:00Z")?,
                is_merge: false,
            }],
            commit_selection: ViewerCommitSelection::None,
            footer: ViewerFooter {
                command: "gtl diff".to_owned(),
            },
            exclusions: None,
        };
        Ok((view, workspace))
    }

    #[test]
    fn artifact_workspace_renders_scoped_enhancement_contract() -> TestResult {
        let (view, workspace) = artifact_view(7)?;
        let html = dioxus_ssr::render_element(rsx! {
            ArtifactDiffWorkspace { view, workspace }
        });

        assert!(html.contains(r#"data-gtl-workspace="""#));
        assert!(html.contains(r#"data-gtl-view="7""#));
        assert!(html.contains(r#"data-gtl-files-folded="false""#));
        assert!(html.contains(r#"data-gtl-copy-context="true""#));
        assert_eq!(html.matches(r#"data-gtl-action="filter-files""#).count(), 2);
        assert_eq!(html.matches(r#"data-gtl-action="toggle-files""#).count(), 2);
        assert_eq!(
            html.matches(r#"data-gtl-action="toggle-copy-context""#)
                .count(),
            2
        );
        assert_eq!(html.matches("data-gtl-selected-classes=").count(), 2);
        assert_eq!(html.matches("data-gtl-unselected-classes=").count(), 2);
        assert_eq!(html.matches(r#"data-gtl-file-tree="""#).count(), 2);
        assert_eq!(html.matches(r#"data-gtl-file-directory="""#).count(), 2);
        assert_eq!(html.matches(r#"data-gtl-file-leaf="""#).count(), 2);
        assert_eq!(html.matches(r#"data-gtl-files-empty="""#).count(), 2);
        assert_eq!(
            html.matches(r#"data-file-target="artifact-view-7-file-0""#)
                .count(),
            2
        );
        assert!(html.contains(r#"id="artifact-view-7-file-0""#));
        assert!(html.contains(r#"id="artifact-view-7-files-trigger""#));
        assert!(html.contains(r#"aria-controls="artifact-view-7-files-dialog""#));
        assert!(html.contains(r#"id="artifact-view-7-files-dialog""#));
        assert!(html.contains(r#"data-gtl-dialog-trigger="artifact-view-7-files-trigger""#,));
        assert!(html.contains(r#"data-gtl-action="close-dialog""#));
        assert!(html.contains(r#"data-gtl-action="copy-commit""#));
        assert!(
            html.contains(r#"data-gtl-copy-value="0123456789abcdef0123456789abcdef01234567""#,)
        );
        assert!(!html.contains(r#"id="src/<unsafe>.rs""#));
        Ok(())
    }

    #[test]
    fn artifact_targets_are_prefixed_by_typed_tab_ids() -> TestResult {
        let first = ArtifactViewMarkup::new(viewer_tab_id(1)?);
        let second = ArtifactViewMarkup::new(viewer_tab_id(2)?);
        let file = ViewerDiffFileId::for_index(0);

        assert_ne!(
            first.control_id("files-dialog"),
            second.control_id("files-dialog")
        );
        assert_ne!(first.file_target_id(&file), second.file_target_id(&file));
        assert_eq!(first.file_target_id(&file), "artifact-view-1-file-0");
        Ok(())
    }
}
