use dioxus::{dioxus_core::Task, prelude::*};
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    timestamps::MachineTimestamp,
};
use gtl_wire::viewer::{ViewerCommitSelection, ViewerCommitSummary};
use lucide_dioxus::CircleDot;

use crate::shared::{
    browser,
    ui::{
        Badge, BadgeVariant, Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant,
        EmptyNotice, ScrollArea,
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
    let _context = super::use_static_diff_workspace_context(view.into(), commits.into());

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
            class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
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
                selection_active: selected_id.is_some(),
                selection_pending,
                onclear,
            }
            if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                CommitSelectionError { message: message.clone() }
            }
            if view.commit_count == 0 {
                EmptyNotice { "no commits in range" }
            }
            for (commit_index, commit) in workspace.commits.iter().enumerate() {
                {
                    let commit_id = commit.peek().id.clone();
                    let selected = selected_id == Some(&commit_id);
                    rsx! {
                        CommitCard {
                            key: "{commit_id}",
                            commit_index,
                            details_popover_id_prefix: details_popover_id_prefix.clone(),
                            selected,
                            selection_pending,
                            onselect,
                        }
                    }
                }
            }
            if loading {
                p { class: "px-2 py-3 text-center text-ink-3", role: "status", "Loading commits..." }
            } else if let Some(message) = load_error {
                div {
                    class: "mx-1 mt-2 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del",
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
                        class: "mx-auto mt-2",
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
    selection_active: bool,
    selection_pending: bool,
    onclear: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "flex items-start justify-between gap-2",
            div {
                h3 { class: "mx-0.5 mt-1.5 mb-1 font-semibold tracking-wider text-ink-3 uppercase",
                    "{label}"
                }
                CommitPanelHint {}
            }
            if selection_active {
                if let Some(onclear) = onclear {
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                        onclick: move |_| onclear.call(()),
                        "Range"
                    }
                }
            }
        }
    }
}

#[component]
fn CommitPanelHint() -> Element {
    rsx! {
        p { class: "mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-ink-3",
            span { class: "flex-none text-acc", aria_hidden: "true",
                CircleDot { size: 8, fill: "currentColor" }
            }
            "click ID to copy"
        }
    }
}

#[component]
fn CommitSelectionError(message: String) -> Element {
    rsx! {
        p {
            class: "mb-2 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del",
            role: "alert",
            "{message}"
        }
    }
}

const COMMIT_CARD_CLASSES: &str = "relative ml-1.5 w-[calc(100%_-_0.375rem)] rounded-r-sm border-0 border-l-2 py-1.5 pr-2 pl-6 text-left focus-visible:outline-offset-1";
const COMMIT_DETAILS_HOVER_DELAY: std::time::Duration = std::time::Duration::from_millis(350);

#[derive(Default)]
struct CommitDetailsHoverState {
    pointer_inside: bool,
    focus_inside: bool,
    reveal_task: Option<Task>,
}

#[derive(Clone, Copy)]
enum CommitDetailsInteraction {
    Pointer,
    Focus,
}

#[component]
fn CommitCard(
    commit_index: usize,
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
    let hover_state = use_signal(CommitDetailsHoverState::default);
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
            onmouseenter: {
                let popover_id = popover_id.clone();
                move |_| begin_commit_details_interaction(
                    hover_state,
                    CommitDetailsInteraction::Pointer,
                    popover_id.clone(),
                )
            },
            onmouseleave: {
                let popover_id = popover_id.clone();
                move |_| end_commit_details_interaction(
                    hover_state,
                    CommitDetailsInteraction::Pointer,
                    &popover_id,
                )
            },
            onfocusin: {
                let popover_id = popover_id.clone();
                move |_| begin_commit_details_interaction(
                    hover_state,
                    CommitDetailsInteraction::Focus,
                    popover_id.clone(),
                )
            },
            onfocusout: {
                let popover_id = popover_id.clone();
                move |_| end_commit_details_interaction(
                    hover_state,
                    CommitDetailsInteraction::Focus,
                    &popover_id,
                )
            },
            if let Some(onselect) = onselect {
                Button {
                    layout: ButtonLayout::Block,
                    size: ButtonSize::Content,
                    variant: ButtonVariant::Bare,
                    state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                    class: "absolute inset-0 z-2 size-full rounded-r-sm focus-visible:outline-offset-1",
                    aria_label: selection_label,
                    aria_pressed: selected.to_string(),
                    onclick: move |_| onselect.call(id.clone()),
                }
            }
            CommitCardContent { commit, selected, selectable }
            CommitDetailsPopover {
                commit: commit.cloned(),
                id: popover_id.clone(),
                anchor_name,
            }
        }
    }
}

fn begin_commit_details_interaction(
    mut state: Signal<CommitDetailsHoverState>,
    interaction: CommitDetailsInteraction,
    popover_id: String,
) {
    {
        let mut state = state.write();
        let already_active = state.pointer_inside || state.focus_inside;
        match interaction {
            CommitDetailsInteraction::Pointer => state.pointer_inside = true,
            CommitDetailsInteraction::Focus => state.focus_inside = true,
        }
        if already_active {
            return;
        }
        if let Some(task) = state.reveal_task.take() {
            task.cancel();
        }
    }

    let reveal_task = spawn(async move {
        dioxus_sdk_time::sleep(COMMIT_DETAILS_HOVER_DELAY).await;
        browser::show_popover(&popover_id);
    });
    state.write().reveal_task = Some(reveal_task);
}

fn end_commit_details_interaction(
    mut state: Signal<CommitDetailsHoverState>,
    interaction: CommitDetailsInteraction,
    popover_id: &str,
) {
    let mut state = state.write();
    match interaction {
        CommitDetailsInteraction::Pointer => state.pointer_inside = false,
        CommitDetailsInteraction::Focus => state.focus_inside = false,
    }
    if state.pointer_inside || state.focus_inside {
        return;
    }
    if let Some(task) = state.reveal_task.take() {
        task.cancel();
    }
    browser::hide_popover(popover_id);
}

#[component]
fn CommitCardContent(
    commit: ReadStore<ViewerCommitSummary>,
    selected: bool,
    selectable: bool,
) -> Element {
    let commit = commit.read();
    rsx! {
        span {
            class: "relative block",
            class: if selectable { "pointer-events-none" } else { "" },
            CommitTimelineMarker { selected }
            span { class: "mb-1 flex min-w-0 items-center gap-1.5",
                CommitIdButton { id: commit.id.clone() }
                if commit.is_merge {
                    Badge { variant: BadgeVariant::Neutral, "merge" }
                }
                CommitDate { committed_at: commit.committed_at.clone() }
            }
            span { class: "block min-w-0 text-wrap leading-normal text-ink-2", "{commit.subject}" }
        }
    }
}

#[component]
fn CommitDetailsPopover(commit: ViewerCommitSummary, id: String, anchor_name: String) -> Element {
    let abbreviated_id = commit.id.abbreviated(CommitIdAbbreviation::TenCharacters);
    let aria_label = format!("Commit details for {abbreviated_id}");
    let committed_at_display = commit.committed_at.display_minute();
    let committed_at_iso = commit.committed_at.to_string();
    let anchor_style = format!("position-anchor: {anchor_name};");

    rsx! {
        span {
            id,
            class: "fixed inset-auto z-70 m-0 w-[min(19rem,calc(100vw-1rem))] -translate-x-2 border-0 bg-transparent p-0 [position-area:left_span-bottom] [position-try-fallbacks:flip-inline]",
            style: anchor_style,
            popover: "auto",
            role: "tooltip",
            aria_label,
            div { class: "max-h-[min(18rem,calc(100vh-1rem))] overflow-x-hidden overflow-y-auto rounded-panel border border-line-2 bg-surface p-3 text-ink shadow-floating animate-commit-popover-enter motion-reduce:animate-none",
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
    }
}

#[component]
fn CommitTimelineMarker(selected: bool) -> Element {
    rsx! {
        span {
            class: "pointer-events-none absolute top-2 -left-4 flex size-5 items-center justify-center bg-surface",
            class: if selected { "text-acc" } else { "text-line-2" },
            aria_hidden: "true",
            CircleDot { size: 10 }
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
                size: ButtonSize::Inline,
                variant: ButtonVariant::Secondary,
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
        "border-acc bg-acc-soft"
    } else {
        "border-line-2 bg-transparent hover:border-l-acc-line hover:bg-surface-2 active:bg-acc-soft"
    }
}

const fn commit_selection_enabled(commit_count: usize) -> bool {
    commit_count > 1
}

#[cfg(test)]
mod tests {
    use super::commit_selection_enabled;

    #[test]
    fn commit_selection_requires_multiple_commits() {
        assert!(!commit_selection_enabled(0));
        assert!(!commit_selection_enabled(1));
        assert!(commit_selection_enabled(2));
    }
}
