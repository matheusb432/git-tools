use dioxus::prelude::*;
use dx_book::{story, variant};

use crate::shared::ui::{Badge, BadgeVariant};

#[variant(name = "Catalog preview")]
fn preview() -> Element {
    variants()
}

/// All badge variants.
#[variant]
fn variants() -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            Badge { variant: BadgeVariant::Neutral, "Neutral" }
            Badge { variant: BadgeVariant::Accent, "Selected branch" }
            Badge { variant: BadgeVariant::Addition, "+24 lines" }
            Badge { variant: BadgeVariant::Deletion, "-9 lines" }
        }
    }
}

/// Badge component.
#[story(id = "badge", name = "Badge", preview = preview)]
const BADGE_STORY: () = &[variants];
