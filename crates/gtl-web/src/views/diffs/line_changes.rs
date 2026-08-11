use dioxus::prelude::*;

use crate::shared::ui::{Badge, BadgeVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLineChangeKind {
    Added,
    Removed,
}

impl DiffLineChangeKind {
    const fn badge_variant(self) -> BadgeVariant {
        match self {
            Self::Added => BadgeVariant::Addition,
            Self::Removed => BadgeVariant::Deletion,
        }
    }

    const fn text_classes(self) -> &'static str {
        match self {
            Self::Added => "text-add",
            Self::Removed => "text-del",
        }
    }

    fn label(self, count: u64) -> String {
        match self {
            Self::Added => format!("+{count}"),
            Self::Removed => format!("\u{2212}{count}"),
        }
    }
}

#[component]
pub(crate) fn DiffLineChangeBadge(kind: DiffLineChangeKind, count: u64) -> Element {
    rsx! {
        Badge { class: "px-2 py-0.5", variant: kind.badge_variant(), "{kind.label(count)}" }
    }
}

#[component]
pub(crate) fn DiffLineChangeText(kind: DiffLineChangeKind, count: u64) -> Element {
    let classes = kind.text_classes();
    let label = kind.label(count);

    match kind {
        DiffLineChangeKind::Added => rsx! {
            span { class: "{classes}", "data-lines-added": count, "{label}" }
        },
        DiffLineChangeKind::Removed => rsx! {
            span { class: "{classes}", "data-lines-removed": count, "{label}" }
        },
    }
}
