use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::diffs::{CommitId, CommitIdAbbreviation};
use gtl_wire::viewer::{ViewerCommitSelection, ViewerCommitSummary};
use lucide_dioxus::{Check, X};

use crate::shared::{
    browser,
    date_display::DateDisplayTime,
    failure_message::failure_message,
    i18n::{t, use_language},
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
    actions: Option<Element>,

    test_id: Option<String>,
    onselect: Option<EventHandler<CommitId>>,
    #[props(default)] loading: bool,
    load_error: Option<String>,
    #[props(default)] has_more: bool,
    onloadmore: Option<EventHandler<()>>,
) -> Element {
    let language = use_language();
    let workspace = super::use_workspace_context();
    let scroll = super::panel_scroll::use_panel_scroll(super::panel_scroll::Panel::Commits);
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
    let onselect =
        onselect.filter(|_| view.modified_files || commit_selection_enabled(view.commit_count));

    rsx! {
        ScrollArea {
            class: "diff-commits-scroll-panel h-full min-h-0",
            "data-testid": test_id,
            onmounted: scroll.mount,
            onresize: move |_| scroll.restore.call(()),
            onscroll: move |event: ScrollEvent| {
                scroll.save.call(event.clone());
                if has_more
                    && scroll_is_near_bottom(&event.data())
                    && let Some(onloadmore) = onloadmore
                {
                    onloadmore.call(());
                }
            },
            CommitsPanelHeader { actions }
            if let ViewerCommitSelection::Error { failure, .. } = &view.commit_selection {
                CommitSelectionError { message: failure_message(failure, language) }
            }
            if view.commit_count == 0 {
                EmptyNotice { class: "m-3 compact:m-2.5", {t!(language, "commits-empty")} }
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
                p { class: "diff-commits-loading px-3 py-3", role: "status",
                    {t!(language, "commits-loading")}
                }
            } else if let Some(message) = load_error {
                div { class: "diff-commits-error", role: "alert",
                    p { "{message}" }
                    if let Some(onloadmore) = onloadmore {
                        Button {
                            class: "mt-2",
                            size: ButtonSize::Small,
                            variant: ButtonVariant::Failure,
                            onclick: move |_| onloadmore.call(()),
                            {t!(language, "action-retry")}
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
                        {t!(language, "commits-load-more")}
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
fn CommitsPanelHeader(actions: Option<Element>) -> Element {
    let language = use_language();
    rsx! {
        header { class: "diff-commits-header px-3 py-2 compact:px-2.5",
            div { class: "flex items-center justify-between gap-2",
                div {
                    h3 { class: "diff-commits-heading m-0 text-sm font-semibold leading-snug",
                        {t!(language, "workspace-commits")}
                    }
                }
                {actions}
            }
        }
    }
}

#[component]
fn CommitSelectionError(message: String) -> Element {
    rsx! {
        p { class: "diff-commits-selection-error", role: "alert", "{message}" }
    }
}

const COMMIT_CARD_CLASSES: &str = "diff-commit-card w-full px-3 py-3 compact:px-2.5";
#[component]
fn CommitCard(
    commit_index: usize,

    details_popover_id_prefix: String,
    selected: bool,
    selection_pending: bool,
    onselect: Option<EventHandler<CommitId>>,
) -> Element {
    let language = use_language();
    let workspace = super::use_workspace_context();
    let Some(commit) = workspace.commits.get(commit_index) else {
        return rsx! {};
    };
    let commit: ReadStore<ViewerCommitSummary> = commit.into();
    let (id, selection_label, popover_id) = commit.with(|commit| {
        let abbreviated_id = commit.id.abbreviated(CommitIdAbbreviation::TenCharacters);
        (
            commit.id.clone(),
            t!(
                language,
                "commits-select",
                commit = abbreviated_id,
                subject = commit.subject.as_str()
            ),
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
                active: (hover.active)(),
            }
        }
    }
}

#[component]
fn CommitCardContent(commit: ReadStore<ViewerCommitSummary>, selectable: bool) -> Element {
    let language = use_language();
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
                    Badge { variant: BadgeVariant::Neutral, {t!(language, "commits-merge")} }
                }
                DateDisplayTime {
                    class: "ml-auto truncate text-ink-3 text-xs tabular-nums",
                    timestamp: commit.committed_at.clone(),
                }
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
    let aria_label = t!(
        use_language(),
        "commits-details-for",
        commit = abbreviated_id
    );
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
    let language = use_language();
    let commit = commit.read();
    rsx! {
        header { class: "diff-commit-details-header gap-2",
            p { class: "text-xs font-semibold tracking-widest text-ink-3 uppercase",
                {t!(language, "commits-details")}
            }
            if commit.is_merge {
                Badge { variant: BadgeVariant::Neutral, {t!(language, "commits-merge")} }
            }
        }
        h4 { class: "mt-2 text-sm font-semibold leading-snug text-ink", "{commit.subject}" }
        if !commit.body.is_empty() {
            p { class: "diff-commit-details-body mt-2 text-xs leading-normal", "{commit.body}" }
        }
        dl { class: "diff-commit-details-metadata mt-3",
            div { class: "grid gap-1 py-2",
                dt { class: "text-xs font-semibold text-ink-3", {t!(language, "commits-date")} }
                dd { class: "m-0 min-w-0",
                    DateDisplayTime {
                        class: "block text-xs tabular-nums text-ink",
                        timestamp: commit.committed_at.clone(),
                    }
                }
            }
            div { class: "grid gap-1 pt-2",
                dt { class: "text-xs font-semibold text-ink-3", {t!(language, "commits-id")} }
                dd { class: "m-0 min-w-0",
                    code { class: "block break-all text-xs text-ink", "{commit.id}" }
                }
            }
        }
    }
}

#[component]
fn CommitIdButton(id: CommitId) -> Element {
    let language = use_language();
    let abbreviated_id = id.abbreviated(CommitIdAbbreviation::TenCharacters);
    let mut copy_result = use_signal(|| None::<bool>);
    let mut copy_action = use_action(move || {
        let id = id.clone();
        async move {
            copy_result.set(Some(browser::copy_text(id.as_ref()).await));
            dioxus_sdk_time::sleep(Duration::from_millis(1_500)).await;
            copy_result.set(None);
            Ok::<(), std::convert::Infallible>(())
        }
    });
    let copy_feedback = match copy_result() {
        Some(true) => t!(language, "copy-copied"),
        Some(false) => t!(language, "copy-failed"),
        None => String::new(),
    };
    let copy_title = if copy_feedback.is_empty() {
        t!(language, "commits-copy-id")
    } else {
        copy_feedback.clone()
    };

    rsx! {
        span { class: "pointer-events-auto relative z-20 flex flex-none",
            Button {
                class: "inline-flex min-h-5 min-w-[10ch] items-center text-xs font-medium leading-none text-acc hover:text-acc-2 active:text-ink",
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                title: copy_title,
                aria_label: t!(language, "commits-copy-id"),
                onclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    copy_result.set(None);
                    copy_action.call();
                },
                match copy_result() {
                    Some(true) => rsx! {
                        span { class: "inline-flex items-center gap-1 text-add",
                            Check { size: 12, stroke_width: 2 }
                            "{copy_feedback}"
                        }
                    },
                    Some(false) => rsx! {
                        span { class: "inline-flex items-center gap-1 text-del",
                            X { size: 12, stroke_width: 2 }
                            "{copy_feedback}"
                        }
                    },
                    None => rsx! {
                        code { "{abbreviated_id}" }
                    },
                }
            }
            span { class: "sr-only", role: "status", aria_live: "polite", "{copy_feedback}" }
        }
    }
}

const fn commit_card_tone_classes(selected: bool) -> &'static str {
    if selected {
        "bg-acc-soft"
    } else {
        "bg-transparent hover:bg-surface-2"
    }
}

const fn commit_selection_enabled(commit_count: usize) -> bool {
    commit_count > 1
}

#[cfg(test)]
mod tests {
    use super::{COMMIT_CARD_CLASSES, commit_card_tone_classes, commit_selection_enabled};

    #[test]
    fn commit_rows_use_surface_tone_without_timeline_geometry() {
        assert_eq!(commit_card_tone_classes(true), "bg-acc-soft");
        assert_eq!(
            commit_card_tone_classes(false),
            "bg-transparent hover:bg-surface-2"
        );
        assert!(COMMIT_CARD_CLASSES.contains("diff-commit-card w-full px-3 py-3"));
        let stylesheet = include_str!("../../../app/assets/styles/diff-workspace.css");
        let (_, styles) = stylesheet.split_once(".diff-commit-card {").unwrap();
        let styles = styles.split('}').next().unwrap();
        assert!(styles.contains("border-b"));
        assert!(
            styles
                .split_whitespace()
                .all(|class| !class.starts_with("border-l-"))
        );
        assert!(
            styles
                .split_whitespace()
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
