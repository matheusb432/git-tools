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

    const fn prefix(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Removed => "\u{2212}",
        }
    }

    fn label(self, count: u64) -> String {
        let prefix = self.prefix();
        if count > 1_000 {
            format!("{prefix}{}.{}k", count / 1_000, count % 1_000 / 100)
        } else {
            format!("{prefix}{count}")
        }
    }
}

#[component]
pub(crate) fn DiffLineChangeBadge(kind: DiffLineChangeKind, count: u64) -> Element {
    let exact = format!("{}{count}", kind.prefix());
    rsx! {
        Badge {
            class: "px-2 py-0.5",
            variant: kind.badge_variant(),
            title: exact.clone(),
            aria_label: exact,
            "{kind.label(count)}"
        }
    }
}

#[component]
pub(crate) fn DiffLineChangeText(kind: DiffLineChangeKind, count: u64) -> Element {
    let classes = kind.text_classes();
    let label = kind.label(count);
    let exact = format!("{}{count}", kind.prefix());

    match kind {
        DiffLineChangeKind::Added => rsx! {
            span {
                class: "{classes}",
                title: exact.clone(),
                aria_label: exact,
                "data-lines-added": count,
                "{label}"
            }
        },
        DiffLineChangeKind::Removed => rsx! {
            span {
                class: "{classes}",
                title: exact.clone(),
                aria_label: exact,
                "data-lines-removed": count,
                "{label}"
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::DiffLineChangeKind;

    #[test]
    fn line_counts_abbreviate_thousands_without_rounding() {
        for (count, expected) in [
            (0, "+0"),
            (999, "+999"),
            (1_000, "+1000"),
            (1_001, "+1.0k"),
            (1_978, "+1.9k"),
            (1_999, "+1.9k"),
            (2_000, "+2.0k"),
            (10_999, "+10.9k"),
            (u64::MAX, "+18446744073709551.6k"),
        ] {
            assert_eq!(DiffLineChangeKind::Added.label(count), expected);
        }
        assert_eq!(DiffLineChangeKind::Removed.label(1_978), "\u{2212}1.9k");
    }
}
