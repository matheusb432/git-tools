use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_contracts::viewer::{ViewerActiveView, ViewerCommitSelection};
use lucide_dioxus::CircleDot;

use crate::shared::ui::{
    Badge, BadgeVariant, Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, ScrollArea,
};

#[component]
pub(super) fn CommitsPanel(
    view: ViewerActiveView,
    onselect: EventHandler<String>,
    onclear: EventHandler<()>,
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
            div { class: "flex items-start justify-between gap-2",
                div {
                    h3 { class: "mx-0.5 mt-1.5 mb-1 font-semibold tracking-wider text-ink-3 uppercase",
                        "{view.commits_label}"
                    }
                    p { class: "mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-ink-3",
                        span { class: "flex-none text-acc", aria_hidden: "true",
                            CircleDot { size: 8, fill: "currentColor" }
                        }
                        "hash = copy · hover = notes"
                    }
                }
                if selected_sha.is_some() {
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                        onclick: move |_| onclear.call(()),
                        "Range"
                    }
                }
            }
            if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                p {
                    class: "mb-2 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del",
                    role: "alert",
                    "{message}"
                }
            }
            if view.commits.is_empty() {
                p { class: "rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic",
                    "no commits in range"
                }
            }
            for commit in &view.commits {
                {
                    let sha = commit.sha.clone();
                    let selected = selected_sha == Some(commit.sha.as_str());
                    let item_attributes = commit_item_attributes(selected);
                    rsx! {
                        Button {
                            layout: ButtonLayout::Block,
                            size: ButtonSize::Content,
                            variant: ButtonVariant::Bare,
                            state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                            attributes: item_attributes,
                            onclick: move |_| onselect.call(sha.clone()),
                            span {
                                class: "pointer-events-none absolute top-2 -left-4 flex size-5 items-center justify-center bg-surface",
                                class: if selected { "text-acc" } else { "text-line-2" },
                                aria_hidden: "true",
                                CircleDot { size: 10 }
                            }
                            span { class: "mb-1 flex min-w-0 items-center gap-1.5",
                                code {
                                    class: "rounded-sm border px-1.5 py-0.5 text-xs",
                                    class: if selected { "border-acc bg-acc text-bg" } else { "border-acc-line bg-acc-soft text-acc" },
                                    "{commit.abbreviated_sha}"
                                }
                                if commit.is_merge {
                                    Badge { variant: BadgeVariant::Neutral, "merge" }
                                }
                                if !commit.date.is_empty() {
                                    time {
                                        class: "ml-auto truncate text-ink-3 tabular-nums",
                                        datetime: commit.iso.clone(),
                                        title: commit.iso.clone(),
                                        "{commit.date}"
                                    }
                                }
                            }
                            span { class: "block wrap-anywhere leading-normal text-ink-2", "{commit.subject}" }
                        }
                    }
                }
            }
        }
    }
}

fn commit_item_attributes(selected: bool) -> Vec<Attribute> {
    merge_attributes(vec![
        attributes!(button {
            class: "relative ml-1.5 w-[calc(100%_-_0.375rem)] rounded-r-sm border-0 border-l-2 py-1.5 pr-2 pl-6 text-left focus-visible:outline-offset-1",
            aria_pressed: selected.to_string(),
        }),
        if selected {
            attributes!(button {
                class: "border-acc bg-acc-soft"
            })
        } else {
            attributes!(button {
                class: "border-line-2 bg-transparent hover:border-l-acc-line hover:bg-surface-2 active:bg-acc-soft",
            })
        },
    ])
}
