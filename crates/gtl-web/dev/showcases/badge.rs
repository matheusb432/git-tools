use dioxus::prelude::*;
use dx_preview::{preview, showcase};

use crate::shared::ui::{Badge, BadgeVariant};

#[preview(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    variants()
}

/// All badge variants.
#[preview]
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
#[showcase(id = "badge", name = "Badge", thumbnail = thumbnail)]
const BADGE_SHOWCASE: () = &[variants];
