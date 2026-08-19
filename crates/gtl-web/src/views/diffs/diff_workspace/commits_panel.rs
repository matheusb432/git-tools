use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    timestamps::MachineTimestamp,
};
use gtl_wire::viewer::{ViewerActiveView, ViewerCommitSelection, ViewerCommitSummary};
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
    view: ViewerActiveView,
    onselect: Option<EventHandler<CommitId>>,
    onclear: Option<EventHandler<()>>,
) -> Element {
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
    let onselect = onselect.filter(|_| commit_selection_enabled(view.commits.len()));

    rsx! {
        ScrollArea { class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
            CommitsPanelHeader {
                label: view.commits_label.clone(),
                selection_active: selected_id.is_some(),
                selection_pending,
                onclear,
            }
            if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                CommitSelectionError { message: message.clone() }
            }
            if view.commits.is_empty() {
                EmptyNotice { "no commits in range" }
            }
            for commit in &view.commits {
                {
                    let selected = selected_id == Some(&commit.id);
                    rsx! {
                        CommitCard {
                            key: "{commit.id}",
                            commit: commit.clone(),
                            selected,
                            selection_pending,
                            onselect,
                        }
                    }
                }
            }
        }
    }
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

#[component]
fn CommitCard(
    commit: ViewerCommitSummary,
    selected: bool,
    selection_pending: bool,
    onselect: Option<EventHandler<CommitId>>,
) -> Element {
    let title = (!commit.body.is_empty()).then(|| commit.body.clone());
    let tone_classes = commit_card_tone_classes(selected);
    let card_attributes = merge_attributes(vec![
        attributes!(div {
            class: COMMIT_CARD_CLASSES,
        }),
        attributes!(div {
            class: tone_classes,
        }),
    ]);

    if let Some(onselect) = onselect {
        let id = commit.id.clone();
        return rsx! {
            Button {
                layout: ButtonLayout::Block,
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                attributes: card_attributes,
                aria_pressed: selected.to_string(),
                title,
                onclick: move |_| onselect.call(id.clone()),
                CommitCardContent { commit, selected }
            }
        };
    }

    rsx! {
        article {
            class: "{COMMIT_CARD_CLASSES}",
            class: "{tone_classes}",
            title,
            CommitCardContent { commit, selected }
        }
    }
}

#[component]
fn CommitCardContent(commit: ViewerCommitSummary, selected: bool) -> Element {
    rsx! {
        CommitTimelineMarker { selected }
        span { class: "mb-1 flex min-w-0 items-center gap-1.5",
            CommitIdButton { id: commit.id.clone() }
            if commit.is_merge {
                Badge { variant: BadgeVariant::Neutral, "merge" }
            }
            CommitDate { committed_at: commit.committed_at }
        }
        CommitSubject { subject: commit.subject }
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

    rsx! {
        Button {
            size: ButtonSize::Inline,
            variant: ButtonVariant::Secondary,
            title: "Copy commit ID",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                copy_commit_id(id.clone());
            },
            code { "{abbreviated_id}" }
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

#[component]
fn CommitSubject(subject: String) -> Element {
    rsx! {
        span { class: "block text-wrap leading-normal text-ink-2", "{subject}" }
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
