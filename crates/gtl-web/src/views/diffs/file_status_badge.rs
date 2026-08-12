use dioxus::prelude::*;
use gtl_wire::viewer::ViewerFileStatus;

use crate::shared::ui::{Badge, BadgeVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DiffFileStatusBadgeSize {
    Compact,
    #[default]
    Header,
}

#[component]
pub(crate) fn DiffFileStatusBadge(
    status: ViewerFileStatus,
    #[props(default)] size: DiffFileStatusBadgeSize,
) -> Element {
    let size_classes = match size {
        DiffFileStatusBadgeSize::Compact => "size-4 min-h-0! flex-none px-0 leading-none font-bold",
        DiffFileStatusBadgeSize::Header => {
            "size-4 min-h-0! flex-none px-0 text-xs leading-none font-bold"
        }
    };
    let label = status_label(status);

    rsx! {
        Badge {
            class: "{size_classes}",
            variant: badge_variant(status),
            title: label,
            aria_label: label,
            "{status_code(status)}"
        }
    }
}

const fn badge_variant(status: ViewerFileStatus) -> BadgeVariant {
    match status {
        ViewerFileStatus::Added => BadgeVariant::Addition,
        ViewerFileStatus::Deleted => BadgeVariant::Deletion,
        ViewerFileStatus::Renamed => BadgeVariant::Accent,
        ViewerFileStatus::Modified => BadgeVariant::Neutral,
    }
}

const fn status_code(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "A",
        ViewerFileStatus::Deleted => "D",
        ViewerFileStatus::Renamed => "R",
        ViewerFileStatus::Modified => "M",
    }
}

const fn status_label(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "Added file",
        ViewerFileStatus::Deleted => "Deleted file",
        ViewerFileStatus::Renamed => "Renamed file",
        ViewerFileStatus::Modified => "Modified file",
    }
}
