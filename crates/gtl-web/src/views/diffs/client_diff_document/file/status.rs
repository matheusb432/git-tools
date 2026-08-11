use dioxus::prelude::*;
use gtl_contracts::viewer::ViewerFileStatus;

use crate::shared::ui::{Badge, BadgeVariant};

#[component]
pub(super) fn DiffFileStatusBadge(status: ViewerFileStatus) -> Element {
    rsx! {
        Badge {
            class: "size-[15px] min-h-0 flex-none px-0 text-[9.5px] leading-none font-bold",
            variant: badge_variant(status),
            title: status_label(status),
            aria_label: status_label(status),
            "{status_code(status)}"
        }
    }
}

pub(super) const fn file_header_background(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "bg-[color-mix(in_srgb,var(--add-bg)_34%,var(--surface-2))]",
        ViewerFileStatus::Deleted => "bg-[color-mix(in_srgb,var(--del-bg)_34%,var(--surface-2))]",
        ViewerFileStatus::Renamed | ViewerFileStatus::Modified => "bg-surface-2",
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
