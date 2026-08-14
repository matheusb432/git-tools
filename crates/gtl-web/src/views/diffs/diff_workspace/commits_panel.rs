use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
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
pub(super) fn CommitsPanel(
    view: ViewerActiveView,
    onselect: Option<EventHandler<String>>,
    onclear: Option<EventHandler<()>>,
) -> Element {
    let selected_sha = match &view.commit_selection {
        ViewerCommitSelection::None => None,
        ViewerCommitSelection::Pending { sha }
        | ViewerCommitSelection::Ready { sha }
        | ViewerCommitSelection::Error { sha, .. } => Some(sha.as_str()),
    };
    let selection_pending = matches!(
        &view.commit_selection,
        ViewerCommitSelection::Pending { .. }
    );

    rsx! {
        ScrollArea { class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
            CommitsPanelHeader {
                label: view.commits_label.clone(),
                selection_active: selected_sha.is_some(),
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
                    let selected = selected_sha == Some(commit.sha.as_str());
                    rsx! {
                        CommitCard {
                            key: "{commit.sha}",
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
            "click hash to copy"
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
    onselect: Option<EventHandler<String>>,
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

    // TODO: remove if `onselect` does not meaningfully change view state. if it's null, just keep
    // the button disabled.
    if let Some(onselect) = onselect {
        let sha = commit.sha.clone();
        return rsx! {
            Button {
                layout: ButtonLayout::Block,
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                attributes: card_attributes,
                aria_pressed: selected.to_string(),
                title,
                onclick: move |_| onselect.call(sha.clone()),
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
            CommitHash {
                sha: commit.sha.clone(),
                abbreviated_sha: commit.abbreviated_sha.clone(),
                selected,
            }
            if commit.is_merge {
                Badge { variant: BadgeVariant::Neutral, "merge" }
            }
            if !commit.date.is_empty() {
                CommitDate { date: commit.date.clone(), iso: commit.iso.clone() }
            }
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
fn CommitHash(sha: String, abbreviated_sha: String, selected: bool) -> Element {
    fn copy_commit_hash(sha: String) {
        spawn(async move {
            browser::copy_text(&sha).await;
            // TODO: add toast notif upon completion
        });
    }

    rsx! {
        Button {
            size: ButtonSize::Inline,
            variant: ButtonVariant::Secondary,
            title: "Copy hash",
            onclick: move |e: Event<MouseData>| {
                e.stop_propagation();
                copy_commit_hash(sha.clone());
            },
            code { "{abbreviated_sha}" }
        }
    }
}

#[component]
fn CommitDate(date: String, iso: String) -> Element {
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
