use std::{convert::Infallible, error::Error, path::PathBuf, time::Duration};

use dioxus::prelude::*;
use dx_preview::{preview, showcase};
use gtl_models::{
    diffs::{CommitId, DiffLineCount},
    git::{BranchName, GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId},
};
use gtl_wire::viewer::{
    ViewerActiveView, ViewerCodeLine, ViewerCodeSpan, ViewerCommandLine, ViewerCommitSelection,
    ViewerCommitSummary, ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileRows,
    ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerRenderOptions, ViewerRows, ViewerTab,
    ViewerTabKind, ViewerTabState, ViewerUnifiedRow, ViewerUnifiedSourceRow, ViewerViewIdentity,
};
use lucide_dioxus::{Check, Ellipsis, History, Settings};

use crate::{
    entities::diffs::{ClientDiffWorkspace, static_diff_workspace},
    shared::ui::{
        Button, ButtonSize, ButtonState, ButtonType, Checkbox, FieldLabel, IconPopover,
        MENU_ACTION_HOST_CLASSES, MenuActionContent, ScrollArea, ScrollAreaVariant, Select,
        SelectOption, ViewerTabItem,
    },
    views::diffs::diff_workspace::PreviewDiffWorkspace,
};

const LAYOUT_OPTIONS: &[(&str, &str)] = &[
    ("", "Choose a layout"),
    ("unified", "Unified"),
    ("split", "Side by side"),
];
const DENSITY_OPTIONS: &[(&str, &str)] = &[
    ("", "Choose a view"),
    ("compact", "Changes only"),
    ("full", "Full file"),
];

#[preview(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-panel border border-line bg-bg",
            div { class: "flex items-center gap-2 border-b border-line bg-surface px-3 py-2",
                span { class: "font-semibold text-ink", "~/git-tools" }
                span { class: "ml-auto text-ink-3", "main -> 9f904e7" }
            }
            div { class: "grid grid-cols-[4rem_1fr] text-xs",
                div { class: "border-r border-line bg-surface-2 p-2 text-ink-3", "3 files" }
                div { class: "space-y-1 p-2",
                    div { class: "h-2 w-4/5 rounded-sm bg-del-bg" }
                    div { class: "h-2 w-3/5 rounded-sm bg-add-bg" }
                    div { class: "h-2 w-2/3 rounded-sm bg-surface-2" }
                }
            }
        }
    }
}

/// Wide viewer composed from production viewer components.
#[preview(name = "Desktop viewer")]
fn desktop_viewer() -> Element {
    rsx! {
        ViewerPreview {}
    }
}

/// Phone viewer with a lean titlebar and dedicated Files and Commits navigation.
#[preview(name = "Mobile viewer")]
fn mobile_viewer() -> Element {
    rsx! {
        ViewerPreview { mobile: true }
    }
}

/// Editable staged settings with stable inline validation.
#[preview(name = "Settings form")]
fn settings_form() -> Element {
    rsx! {
        SettingsMock {}
    }
}

#[component]
fn ViewerPreview(#[props(default)] mobile: bool) -> Element {
    let fixture = match preview_fixture() {
        Ok(fixture) => fixture,
        Err(error) => {
            return rsx! {
                p { role: "alert", "Preview unavailable: {error}" }
            };
        }
    };
    let shell_classes = if mobile {
        "mx-auto flex h-[844px] w-[390px] max-w-full flex-col overflow-hidden rounded-panel border border-line bg-bg shadow-floating"
    } else {
        "mx-auto flex h-[760px] min-w-[70rem] max-w-[90rem] flex-col overflow-hidden rounded-panel border border-line bg-bg shadow-floating"
    };

    rsx! {
        section {
            class: "{shell_classes}",
            aria_label: if mobile { "Mobile viewer redesign mockup" } else { "Desktop viewer redesign mockup" },
            PreviewApplicationTabs { tabs: fixture.tabs, mobile }
            div { class: "min-h-0 flex-1 overflow-hidden",
                PreviewDiffWorkspace {
                    view: fixture.view,
                    workspace: fixture.workspace,
                    mobile,
                }
            }
        }
    }
}

#[component]
fn PreviewApplicationTabs(tabs: Vec<ViewerTab>, mobile: bool) -> Element {
    let initial_tab_id = tabs.first().map(|tab| tab.id);
    let mut active_tab_id = use_signal(move || initial_tab_id);
    let menu_id = if mobile {
        "preview-mobile-viewer-menu"
    } else {
        "preview-desktop-viewer-menu"
    };

    rsx! {
        nav {
            class: if mobile { "z-70 flex min-w-0 shrink-0 items-end gap-1 border-b border-line bg-surface px-2 pt-2" } else { "z-70 flex min-w-0 shrink-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2" },
            aria_label: "Viewer navigation",
            ScrollArea {
                variant: ScrollAreaVariant::Rail,
                class: "flex min-w-0 flex-1 items-end gap-0 overflow-x-auto",
                role: "tablist",
                aria_label: "Open diffs",
                for (index, tab) in tabs.iter().enumerate() {
                    if !mobile || index == 0 {
                        {
                            let tab_id = tab.id;
                            rsx! {
                                ViewerTabItem {
                                    key: "{tab.id}",
                                    tab: tab.clone(),
                                    active: active_tab_id() == Some(tab_id),
                                    onactivate: move |_| active_tab_id.set(Some(tab_id)),
                                    onkeydown: move |_| {},
                                    onclose: move |_| {},
                                }
                            }
                        }
                    }
                }
            }
            IconPopover {
                id: menu_id,
                aria_label: "Viewer menu",
                trigger_size: if mobile { ButtonSize::IconTouch } else { ButtonSize::IconSmall },
                icon: rsx! {
                    Ellipsis { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    button { class: MENU_ACTION_HOST_CLASSES, r#type: "button",
                        MenuActionContent {
                            icon: rsx! {
                                History { size: 15 }
                            },
                            label: "History",
                            description: "Browse saved renders",
                        }
                    }
                    button { class: MENU_ACTION_HOST_CLASSES, r#type: "button",
                        MenuActionContent {
                            icon: rsx! {
                                Settings { size: 15 }
                            },
                            label: "Settings",
                            description: "Viewer defaults",
                        }
                    }
                }
            }
        }
    }
}

struct PreviewFixture {
    tabs: Vec<ViewerTab>,
    view: ViewerActiveView,
    workspace: ClientDiffWorkspace,
}

type PreviewResult<T> = Result<T, Box<dyn Error>>;

fn preview_fixture() -> PreviewResult<PreviewFixture> {
    let tab_id = ViewerTabId::try_new(1)?;
    let identity = ViewerViewIdentity {
        tab_id,
        range_generation: ViewerRangeGeneration::new(1),
        selection_generation: ViewerSelectionGeneration::new(1),
        render_options: ViewerRenderOptions {
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
    };
    let (files, workspace) = preview_files(identity)?;
    let commits = preview_commits()?;
    let view = ViewerActiveView {
        identity,
        title: "diff".to_owned(),
        repository_name: ProjectName::try_from("git-tools")?,
        branch: GitHead::Branch(BranchName::main()),
        upstream: GitRevision::main(),
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "main..9f904e7331".to_owned(),
            trail: String::new(),
        },
        files,
        commits_label: "4 commits".to_owned(),
        commit_count: commits.len(),
        commits,
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "gtl diff main".to_owned(),
        },
        exclusions: None,
    };
    let tabs = vec![
        ViewerTab {
            id: tab_id,
            label: "git-tools".to_owned(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        },
        ViewerTab {
            id: ViewerTabId::try_new(2)?,
            label: "config-cleanup".to_owned(),
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        },
    ];

    Ok(PreviewFixture {
        tabs,
        view,
        workspace,
    })
}

fn preview_files(
    identity: ViewerViewIdentity,
) -> PreviewResult<(Vec<ViewerFileSummary>, ClientDiffWorkspace)> {
    let files = vec![
        preview_file(
            0,
            "crates/gtl-web/src/views/user_settings.rs",
            6,
            1,
            ViewerFileStatus::Modified,
            true,
        )?,
        preview_file(
            1,
            "crates/gtl-web/src/shared/ui/button.rs",
            14,
            0,
            ViewerFileStatus::Modified,
            false,
        )?,
        preview_file(
            2,
            "docs/agents/dioxus-web.md",
            4,
            3,
            ViewerFileStatus::Modified,
            false,
        )?,
    ];
    let workspace = static_diff_workspace(
        identity,
        vec![
            (files[0].clone(), settings_rows()),
            (files[1].clone(), button_rows()),
            (files[2].clone(), documentation_rows()),
        ],
    );
    Ok((files, workspace))
}

fn preview_commits() -> PreviewResult<Vec<ViewerCommitSummary>> {
    Ok(vec![
        preview_commit(
            "9f904e7331d8a61bb9b75143ad54ee876e544a01",
            "add editable viewer settings",
            "2026-09-01T13:42:00Z",
            false,
        )?,
        preview_commit(
            "703de21ad724f91a49c31d747f783bc955bddc72",
            "stream viewer row updates",
            "2026-09-01T12:18:00Z",
            false,
        )?,
        preview_commit(
            "18b63ab1f50e84fc496880489ef871dc1ae4fbd1",
            "render unified file headers",
            "2026-08-31T19:03:00Z",
            false,
        )?,
        preview_commit(
            "ae8141c2fd3b38c8aca912007df2f35fa10d5e39",
            "track project exclusions",
            "2026-08-31T16:27:00Z",
            true,
        )?,
    ])
}

fn preview_file(
    index: usize,
    path: &str,
    added: u64,
    removed: u64,
    status: ViewerFileStatus,
    initially_expanded: bool,
) -> PreviewResult<ViewerFileSummary> {
    Ok(ViewerFileSummary {
        id: ViewerDiffFileId::for_index(index),
        path: RepositoryRelativePath::try_new(PathBuf::from(path))?,
        absolute_path: AbsoluteFilePath::try_new(PathBuf::from(format!("/repo/{path}")))?,
        anchor_id: format!("preview-file-{index}"),
        added: DiffLineCount::new(added),
        removed: DiffLineCount::new(removed),
        status,
        can_open_in_editor: true,
        initially_expanded,
    })
}

fn preview_commit(
    id: &str,
    subject: &str,
    committed_at: &str,
    is_merge: bool,
) -> PreviewResult<ViewerCommitSummary> {
    Ok(ViewerCommitSummary {
        id: CommitId::try_from(id)?,
        subject: subject.to_owned(),
        body: "Preview commit details.".to_owned(),
        committed_at: MachineTimestamp::try_from(committed_at)?,
        is_merge,
    })
}

fn settings_rows() -> ViewerFileRows {
    unified_rows(vec![
        ViewerUnifiedRow::Hunk("@@ -18,6 +18,11 @@".to_owned()),
        ViewerUnifiedRow::Context(source_row(
            "pub fn resolve_viewer_settings() -> ViewerSettings {",
            Some(18),
            Some(18),
        )),
        ViewerUnifiedRow::Removed(source_row(
            "    let layout = ViewerLayout::Unified;",
            Some(19),
            None,
        )),
        ViewerUnifiedRow::Added(source_row(
            "    let layout = configured_layout();",
            None,
            Some(19),
        )),
        ViewerUnifiedRow::Context(source_row("    ViewerSettings {", Some(20), Some(20))),
        ViewerUnifiedRow::Added(source_row(
            "        copy_with_context: true,",
            None,
            Some(22),
        )),
        ViewerUnifiedRow::Context(source_row("    }", Some(23), Some(23))),
    ])
}

fn button_rows() -> ViewerFileRows {
    unified_rows(vec![
        ViewerUnifiedRow::Hunk("@@ -42,6 +42,8 @@".to_owned()),
        ViewerUnifiedRow::Added(source_row("    Submit,", None, Some(43))),
        ViewerUnifiedRow::Added(source_row("    IconTouch,", None, Some(44))),
    ])
}

fn documentation_rows() -> ViewerFileRows {
    unified_rows(vec![
        ViewerUnifiedRow::Hunk("@@ -12,3 +12,4 @@".to_owned()),
        ViewerUnifiedRow::Context(source_row(
            "- Keep shared UI domain-free.",
            Some(12),
            Some(12),
        )),
        ViewerUnifiedRow::Added(source_row(
            "- Showcases compose production components.",
            None,
            Some(13),
        )),
    ])
}

fn unified_rows(rows: Vec<ViewerUnifiedRow>) -> ViewerFileRows {
    ViewerFileRows {
        rows: ViewerRows::Unified(rows),
        line_number_digits: 2,
    }
}

fn source_row(
    text: &str,
    old_line_number: Option<u32>,
    new_line_number: Option<u32>,
) -> ViewerUnifiedSourceRow {
    ViewerUnifiedSourceRow {
        old_line_number,
        new_line_number,
        code: ViewerCodeLine {
            text: text.to_owned(),
            spans: vec![ViewerCodeSpan {
                byte_start: 0,
                byte_end: text.len(),
                syntax_class: None,
                changed: false,
            }],
            long_line_character_count: None,
        },
    }
}

#[component]
fn SettingsMock() -> Element {
    rsx! {
        main {
            class: "mx-auto h-[800px] w-full max-w-5xl overflow-auto rounded-panel border border-line bg-bg px-4 py-5 shadow-floating sm:px-6",
            aria_label: "Editable settings redesign mockup",
            div { class: "grid gap-5",
                header { class: "border-b border-line pb-4",
                    div { class: "flex items-center gap-2 text-acc",
                        Settings { size: 16 }
                        p { class: "font-semibold tracking-widest uppercase", "Viewer preferences" }
                    }
                    h1 { class: "mt-1 text-lg font-semibold tracking-tight text-ink",
                        "User settings"
                    }
                    p { class: "mt-1 max-w-2xl leading-5 text-ink-2",
                        "Choose viewer defaults, then submit to save them."
                    }
                }
                SettingsEditableForm {}
                SettingsResolved {}
                SettingsProjects {}
            }
        }
    }
}

#[component]
fn SettingsEditableForm() -> Element {
    let mut layout = use_signal(|| "split".to_owned());
    let mut density = use_signal(|| "compact".to_owned());
    let mut copy_with_context = use_signal(|| true);
    let mut submission_attempted = use_signal(|| false);
    let mut pending = use_signal(|| false);
    let mut saved = use_signal(|| false);
    let mut finish_save = use_action(move || async move {
        dioxus_sdk_time::sleep(Duration::from_millis(650)).await;
        pending.set(false);
        saved.set(true);
        Ok::<(), Infallible>(())
    });
    let layout_value = layout();
    let density_value = density();
    let layout_error = choice_error(
        submission_attempted(),
        &layout_value,
        "Choose a diff layout.",
    );
    let density_error = choice_error(
        submission_attempted(),
        &density_value,
        "Choose a diff view.",
    );
    let valid = layout_error.is_none() && density_error.is_none();

    rsx! {
        form {
            novalidate: true,
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                submission_attempted.set(true);
                saved.set(false);
                if valid {
                    pending.set(true);
                    finish_save.call();
                }
            },
            section {
                class: "overflow-hidden rounded-panel border border-line bg-surface",
                aria_label: "Editable viewer settings",
                header { class: "border-b border-line bg-surface-2 px-4 py-3",
                    h2 { class: "font-semibold text-ink", "Diff display" }
                    p { class: "mt-0.5 text-xs leading-5 text-ink-3",
                        "These defaults apply to viewer sessions and raw artifacts."
                    }
                }
                div { class: "flex flex-col gap-4 p-4",
                    div { class: "flex flex-col gap-1.5",
                        FieldLabel {
                            for_id: "mock-layout",
                            label: "Layout",
                            hint: "Choose how old and new lines share the diff canvas.",
                        }
                        Select {
                            id: "mock-layout",
                            name: "layout",
                            aria_label: "Layout",
                            value: layout_value,
                            options: select_options(LAYOUT_OPTIONS),
                            error: layout_error,
                            disabled: pending(),
                            required: true,
                            onchange: move |event: FormEvent| layout.set(event.value()),
                        }
                    }
                    div { class: "flex flex-col gap-1.5",
                        FieldLabel {
                            for_id: "mock-density",
                            label: "View",
                            hint: "Limit the document to changed regions or show the full file.",
                        }
                        Select {
                            id: "mock-density",
                            name: "density",
                            aria_label: "View",
                            value: density_value,
                            options: select_options(DENSITY_OPTIONS),
                            error: density_error,
                            disabled: pending(),
                            required: true,
                            onchange: move |event: FormEvent| density.set(event.value()),
                        }
                    }
                    div { class: "flex flex-col gap-1.5",
                        FieldLabel {
                            for_id: "mock-copy-context",
                            label: "Copied diff lines",
                        }
                        Checkbox {
                            id: "mock-copy-context",
                            checked: copy_with_context(),
                            disabled: pending(),
                            onchange: move |event: FormEvent| copy_with_context.set(event.checked()),
                            div { class: "min-w-0",
                                span { class: "block font-semibold text-ink", "Copy with context" }
                                span { class: "mt-0.5 block text-xs leading-5 text-ink-3",
                                    "Prepend the path and selected line range when copying diff lines."
                                }
                            }
                        }
                    }
                }
                footer { class: "flex min-h-16 items-center justify-between gap-3 border-t border-line bg-surface-2 px-4 py-3",
                    p {
                        class: "text-xs text-add",
                        role: "status",
                        aria_live: "polite",
                        if saved() {
                            span { class: "inline-flex items-center gap-1.5",
                                Check { size: 14 }
                                "Settings saved"
                            }
                        }
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        state: if pending() { ButtonState::Loading } else { ButtonState::Enabled },
                        "Save settings"
                    }
                }
            }
        }
    }
}

fn choice_error(attempted: bool, value: &str, message: &str) -> Option<String> {
    (attempted && value.is_empty()).then(|| message.to_owned())
}

fn select_options(options: &[(&str, &str)]) -> Vec<SelectOption> {
    options
        .iter()
        .map(|(value, label)| SelectOption::new(*value, *label))
        .collect()
}

#[component]
fn SettingsResolved() -> Element {
    rsx! {
        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Resolved viewer settings",
            header { class: "border-b border-line bg-surface-2 px-4 py-3",
                h2 { class: "font-semibold text-ink", "Resolved configuration" }
                p { class: "mt-0.5 text-xs text-ink-3", "Current sources and effective values." }
            }
            dl { class: "divide-y divide-line",
                SettingsRow {
                    term: "Configuration file",
                    value: "~/.config/git-tools/config.toml",
                }
                SettingsRow { term: "Effective theme", value: "Dark" }
                SettingsRow { term: "Push confirmation", value: "Required" }
                SettingsRow { term: "Default diff exclusions", value: "*.lock, *.snap" }
            }
        }
    }
}

#[component]
fn SettingsProjects() -> Element {
    rsx! {
        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Project exclusions",
            header { class: "border-b border-line bg-surface-2 px-4 py-3",
                h2 { class: "font-semibold text-ink", "Project exclusions" }
                p { class: "mt-0.5 text-xs text-ink-3", "Repository-specific extension filters." }
            }
            dl { class: "divide-y divide-line",
                SettingsRow { term: "git-tools", value: "*.lock, *.snap" }
                SettingsRow { term: "sample_project", value: "*.wasm" }
            }
        }
    }
}

#[component]
fn SettingsRow(term: String, value: String) -> Element {
    rsx! {
        div { class: "grid gap-2 px-4 py-3 sm:grid-cols-[14rem_minmax(0,1fr)]",
            dt { class: "font-semibold text-ink-2", "{term}" }
            dd { class: "m-0 min-w-0 break-words text-ink", "{value}" }
        }
    }
}

/// Viewer and settings redesign proposal.
#[showcase(
    id = "viewer-settings-redesign",
    name = "Viewer settings redesign",
    thumbnail = thumbnail
)]
const VIEWER_SETTINGS_REDESIGN_SHOWCASE: () = &[desktop_viewer, mobile_viewer, settings_form];

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::{ViewerPreview, choice_error};

    #[test]
    fn validation_appears_only_after_submission() {
        assert_eq!(choice_error(false, "", "Required"), None);
        assert_eq!(
            choice_error(true, "", "Required"),
            Some("Required".to_owned())
        );
        assert_eq!(choice_error(true, "split", "Required"), None);
    }

    #[test]
    fn desktop_preview_keeps_production_viewer_interactions() {
        let html = dioxus_ssr::render_element(rsx! {
            ViewerPreview {}
        });

        assert!(html.contains("min-w-24 max-w-56 shrink-0"));
        assert!(html.contains("border-acc bg-surface-2"));
        assert!(html.contains(r#"data-gtl-diff-file="""#));
        assert!(html.contains(r#"data-gtl-action="copy-commit""#));
        assert!(html.contains(r#"aria-pressed="false""#));
        assert!(html.contains("Collapse all"));
        assert!(!html.contains(">+ context<"));
        assert!(!html.contains(">Display<"));
    }

    #[test]
    fn mobile_preview_omits_repository_heading_and_keeps_panel_actions() {
        let html = dioxus_ssr::render_element(rsx! {
            ViewerPreview { mobile: true }
        });

        assert!(!html.contains("~/"));
        assert!(html.contains(r#"aria-label="Changed files""#));
        assert!(html.contains(r#"aria-label="Commits""#));
        assert!(html.contains("inline-flex size-5 flex-none items-center justify-center"));
        assert!(html.contains(r#"data-gtl-action="copy-commit""#));
        assert!(html.contains(r#"data-gtl-diff-file="""#));
    }
}
