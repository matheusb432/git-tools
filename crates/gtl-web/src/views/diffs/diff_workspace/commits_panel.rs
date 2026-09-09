use dioxus::prelude::*;
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    timestamps::MachineTimestamp,
};
use gtl_wire::viewer::{ViewerCommitSelection, ViewerCommitSummary};

use crate::shared::{
    browser,
    ui::{
        Badge, BadgeVariant, Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant,
        EmptyNotice, HoverPopover, ScrollArea, use_hover_popover,
    },
};

#[component]
pub fn CommitsPanel(
    mut view: gtl_wire::viewer::ViewerActiveView,
    test_id: Option<String>,
    onselect: Option<EventHandler<CommitId>>,
    onclear: Option<EventHandler<()>>,
    #[props(default)] loading: bool,
    load_error: Option<String>,
    #[props(default)] has_more: bool,
    onloadmore: Option<EventHandler<()>>,
) -> Element {
    let commits = std::mem::take(&mut view.commits);
    let view = use_signal(move || view);
    let commits = use_store(move || commits);
    let _context = super::use_static_diff_workspace_context(view.into(), commits.into(), false);

    rsx! {
        WorkspaceCommitsPanel {
            details_popover_id_prefix: "standalone-commits-panel",
            test_id,
            onselect,
            onclear,
            loading,
            load_error,
            has_more,
            onloadmore,
        }
    }
}

#[component]
pub(super) fn WorkspaceCommitsPanel(
    details_popover_id_prefix: String,
    #[props(default)] artifact: bool,
    test_id: Option<String>,
    onselect: Option<EventHandler<CommitId>>,
    onclear: Option<EventHandler<()>>,
    #[props(default)] loading: bool,
    load_error: Option<String>,
    #[props(default)] has_more: bool,
    onloadmore: Option<EventHandler<()>>,
) -> Element {
    let workspace = super::use_workspace_context();
    let view = workspace.view.read();
    let selected_id = match &view.commit_selection {
        ViewerCommitSelection::None => None,
        ViewerCommitSelection::Pending { id }
        | ViewerCommitSelection::Ready { id }
        | ViewerCommitSelection::Error { id, .. } => Some(id),
    };
    let selection_pending = matches!(
        &view.commit_selection,
        ViewerCommitSelection::Pending { .. }
    );
    let onselect = onselect.filter(|_| commit_selection_enabled(view.commit_count));

    rsx! {
        ScrollArea {
            class: "h-full min-h-0 overflow-auto bg-surface",
            "data-testid": test_id,
            onscroll: move |event: ScrollEvent| {
                if has_more
                    && scroll_is_near_bottom(&event.data())
                    && let Some(onloadmore) = onloadmore
                {
                    onloadmore.call(());
                }
            },
            CommitsPanelHeader {
                label: view.commits_label.clone(),
                commit_count: view.commit_count,
                selection_active: selected_id.is_some(),
                selection_pending,
                onclear,
            }
            if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                CommitSelectionError { message: message.clone() }
            }
            if view.commit_count == 0 {
                EmptyNotice { class: "m-3 compact:m-2.5", "no commits in range" }
            }
            for (commit_index, commit) in workspace.commits.iter().enumerate() {
                {
                    let commit_id = commit.peek().id.clone();
                    let selected = selected_id == Some(&commit_id);
                    rsx! {
                        CommitCard {
                            key: "{commit_id}",
                            commit_index,
                            artifact,
                            details_popover_id_prefix: details_popover_id_prefix.clone(),
                            selected,
                            selection_pending,
                            onselect,
                        }
                    }
                }
            }
            if loading {
                p { class: "px-3 py-3 text-center text-ink-3", role: "status", "Loading commits..." }
            } else if let Some(message) = load_error {
                div {
                    class: "mx-3 mt-3 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del compact:mx-2.5",
                    role: "alert",
                    p { "{message}" }
                    if let Some(onloadmore) = onloadmore {
                        Button {
                            class: "mt-2",
                            size: ButtonSize::Small,
                            variant: ButtonVariant::Failure,
                            onclick: move |_| onloadmore.call(()),
                            "Retry"
                        }
                    }
                }
            } else if has_more {
                if let Some(onloadmore) = onloadmore {
                    Button {
                        class: "mx-auto my-3",
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| onloadmore.call(()),
                        "Load more"
                    }
                }
            }
        }
    }
}

fn scroll_is_near_bottom(scroll: &ScrollData) -> bool {
    const LOAD_AHEAD_PIXELS: f64 = 240.0;

    scroll.scroll_top() + f64::from(scroll.client_height())
        >= f64::from(scroll.scroll_height()) - LOAD_AHEAD_PIXELS
}

#[component]
fn CommitsPanelHeader(
    label: String,
    commit_count: usize,
    selection_active: bool,
    selection_pending: bool,
    onclear: Option<EventHandler<()>>,
) -> Element {
    let heading = commit_panel_heading(&label);
    rsx! {
        header { class: "border-b border-line px-3 py-2 compact:px-2.5",
            div { class: "flex items-start justify-between gap-2",
                div { class: "min-w-0 flex-1",
                    h3 {
                        class: "m-0 flex whitespace-nowrap text-sm font-semibold leading-snug text-ink",
                        title: "{heading}: {commit_count}",
                        span { class: "truncate first-letter:uppercase", "{heading}" }
                        span { class: "flex-none tabular-nums", ": {commit_count}" }
                    }
                }
                if selection_active {
                    if let Some(onclear) = onclear {
                        Button {
                            class: "min-h-7 text-acc hover:text-acc-2 active:text-ink",
                            size: ButtonSize::Content,
                            variant: ButtonVariant::Bare,
                            state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                            onclick: move |_| onclear.call(()),
                            "Range"
                        }
                    }
                }
            }
            CommitPanelHint {}
        }
    }
}

fn commit_panel_heading(label: &str) -> &str {
    label.strip_prefix("# ").unwrap_or(label)
}

#[component]
fn CommitPanelHint() -> Element {
    rsx! {
        p { class: "mt-1 mb-0 text-xs text-ink-3", "click ID to copy" }
    }
}

#[component]
fn CommitSelectionError(message: String) -> Element {
    rsx! {
        p {
            class: "m-3 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del compact:m-2.5",
            role: "alert",
            "{message}"
        }
    }
}

const COMMIT_CARD_CLASSES: &str =
    "relative w-full border-0 border-b border-line px-3 py-3 text-left compact:px-2.5";
#[component]
fn CommitCard(
    commit_index: usize,
    artifact: bool,
    details_popover_id_prefix: String,
    selected: bool,
    selection_pending: bool,
    onselect: Option<EventHandler<CommitId>>,
) -> Element {
    let workspace = super::use_workspace_context();
    let Some(commit) = workspace.commits.get(commit_index) else {
        return rsx! {};
    };
    let commit: ReadStore<ViewerCommitSummary> = commit.into();
    let (id, selection_label, popover_id) = commit.with(|commit| {
        let abbreviated_id = commit.id.abbreviated(CommitIdAbbreviation::TenCharacters);
        (
            commit.id.clone(),
            format!("Select commit {abbreviated_id}: {}", commit.subject),
            format!("{details_popover_id_prefix}-{}-details", commit.id.as_ref()),
        )
    });
    let tone_classes = commit_card_tone_classes(selected);
    let selectable = onselect.is_some();
    let hover = use_hover_popover(popover_id.clone());
    let anchor_name = format!("--{popover_id}");
    let anchor_style = format!("anchor-name: {anchor_name};");

    rsx! {
        article {
            class: "{COMMIT_CARD_CLASSES}",
            class: "{tone_classes}",
            style: anchor_style,
            "data-gtl-hover-popover-target": "",
            "data-gtl-hover-popover-id": popover_id.clone(),
            "data-gtl-hover-popover-delay-ms": "350",
            onmouseenter: move |_| hover.pointer_enter.call(()),
            onmouseleave: move |_| hover.pointer_leave.call(()),
            onfocusin: move |_| hover.focus_enter.call(()),
            onfocusout: move |_| hover.focus_leave.call(()),
            if let Some(onselect) = onselect {
                Button {
                    layout: ButtonLayout::Block,
                    size: ButtonSize::Content,
                    variant: ButtonVariant::Bare,
                    state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                    class: "absolute inset-0 z-2 size-full rounded-none focus-visible:-outline-offset-2",
                    aria_label: selection_label,
                    aria_pressed: selected.to_string(),
                    onclick: move |_| onselect.call(id.clone()),
                }
            }
            CommitCardContent { commit, selectable }
            CommitDetailsPopover {
                commit,
                id: popover_id.clone(),
                anchor_name,
                active: artifact || (hover.active)(),
            }
        }
    }
}

#[component]
fn CommitCardContent(commit: ReadStore<ViewerCommitSummary>, selectable: bool) -> Element {
    let commit = commit.read();
    rsx! {
        span {
            class: "relative block",
            class: if selectable { "pointer-events-none" } else { "" },
            span { class: "mb-2 block min-w-0 text-wrap font-medium leading-snug text-ink",
                "{commit.subject}"
            }
            span { class: "flex min-w-0 items-center gap-1.5",
                CommitIdButton { id: commit.id.clone() }
                if commit.is_merge {
                    Badge { variant: BadgeVariant::Neutral, "merge" }
                }
                CommitDate { committed_at: commit.committed_at.clone() }
            }
        }
    }
}

#[component]
fn CommitDetailsPopover(
    commit: ReadStore<ViewerCommitSummary>,
    id: String,
    anchor_name: String,
    active: bool,
) -> Element {
    let abbreviated_id =
        commit.with(|commit| commit.id.abbreviated(CommitIdAbbreviation::TenCharacters));
    let aria_label = format!("Commit details for {abbreviated_id}");
    rsx! {
        HoverPopover { id, anchor_name, aria_label,
            if active {
                CommitDetailsContent { commit }
            }
        }
    }
}

#[component]
fn CommitDetailsContent(commit: ReadStore<ViewerCommitSummary>) -> Element {
    let commit = commit.read();
    let committed_at_display = commit.committed_at.display_minute();
    let committed_at_iso = commit.committed_at.to_string();
    rsx! {
        header { class: "flex items-center justify-between gap-2",
            p { class: "text-xs font-semibold tracking-widest text-ink-3 uppercase",
                "Commit details"
            }
            if commit.is_merge {
                Badge { variant: BadgeVariant::Neutral, "merge" }
            }
        }
        h4 { class: "mt-2 text-sm font-semibold leading-snug text-ink", "{commit.subject}" }
        if !commit.body.is_empty() {
            p { class: "mt-2 whitespace-pre-wrap break-words text-xs leading-normal text-ink-2",
                "{commit.body}"
            }
        }
        dl { class: "mt-3 divide-y divide-line border-t border-line",
            div { class: "grid gap-1 py-2",
                dt { class: "text-xs font-semibold text-ink-3", "Date" }
                dd { class: "m-0 min-w-0",
                    time {
                        class: "block text-xs tabular-nums text-ink",
                        datetime: committed_at_iso,
                        "{committed_at_display}"
                    }
                }
            }
            div { class: "grid gap-1 pt-2",
                dt { class: "text-xs font-semibold text-ink-3", "Commit ID" }
                dd { class: "m-0 min-w-0",
                    code { class: "block break-all text-xs text-ink", "{commit.id}" }
                }
            }
        }
    }
}

#[component]
fn CommitIdButton(id: CommitId) -> Element {
    fn copy_commit_id(id: CommitId) {
        spawn(async move {
            browser::copy_text(id.as_ref()).await;
            // TODO: add toast notif upon completion
        });
    }

    let abbreviated_id = id.abbreviated(CommitIdAbbreviation::TenCharacters);
    let copy_value = id.as_ref().to_owned();

    rsx! {
        span { class: "pointer-events-auto relative z-20 flex flex-none",
            Button {
                class: "min-h-5 text-xs font-medium leading-none text-acc hover:text-acc-2 active:text-ink",
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                title: "Copy commit ID",
                "data-gtl-action": "copy-commit",
                "data-gtl-copy-value": copy_value,
                onclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    copy_commit_id(id.clone());
                },
                code { "{abbreviated_id}" }
            }
        }
    }
}

#[component]
fn CommitDate(committed_at: MachineTimestamp) -> Element {
    let date = committed_at.display_minute();
    let iso = committed_at.to_string();
    rsx! {
        time {
            class: "ml-auto truncate text-ink-3 text-xs tabular-nums",
            datetime: iso.clone(),
            title: iso,
            "{date}"
        }
    }
}

const fn commit_card_tone_classes(selected: bool) -> &'static str {
    if selected {
        "bg-acc-soft"
    } else {
        "bg-transparent hover:bg-surface-2 active:bg-acc-soft"
    }
}

const fn commit_selection_enabled(commit_count: usize) -> bool {
    commit_count > 1
}

#[cfg(test)]
mod tests {
    use super::{
        COMMIT_CARD_CLASSES, commit_card_tone_classes, commit_panel_heading,
        commit_selection_enabled,
    };

    #[test]
    fn commit_heading_omits_the_shell_comment_prefix() {
        assert_eq!(
            commit_panel_heading("# commits in range"),
            "commits in range"
        );
        assert_eq!(commit_panel_heading("4 commits"), "4 commits");
    }

    #[test]
    fn commit_rows_use_surface_tone_without_timeline_geometry() {
        assert_eq!(commit_card_tone_classes(true), "bg-acc-soft");
        assert_eq!(
            commit_card_tone_classes(false),
            "bg-transparent hover:bg-surface-2 active:bg-acc-soft"
        );
        assert!(COMMIT_CARD_CLASSES.contains("border-b"));
        assert!(
            COMMIT_CARD_CLASSES
                .split_ascii_whitespace()
                .all(|class| !class.starts_with("border-l-"))
        );
        assert!(
            COMMIT_CARD_CLASSES
                .split_ascii_whitespace()
                .all(|class| !class.starts_with("rounded"))
        );
    }

    #[test]
    fn commit_selection_requires_multiple_commits() {
        assert!(!commit_selection_enabled(0));
        assert!(!commit_selection_enabled(1));
        assert!(commit_selection_enabled(2));
    }
}
