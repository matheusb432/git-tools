use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::{Badge, BadgeVariant};

pub(super) const VARIANTS: &[StoryVariant] = &[StoryVariant {
    slug: "variants",
    title: "Variants",
    summary: "All badge variants.",
    render: variants,
}];

pub(super) fn preview(context: StoryContext) -> Element {
    variants(context)
}

fn variants(_context: StoryContext) -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            Badge { variant: BadgeVariant::Neutral, "Neutral" }
            Badge { variant: BadgeVariant::Accent, "Selected branch" }
            Badge { variant: BadgeVariant::Addition, "+24 lines" }
            Badge { variant: BadgeVariant::Deletion, "-9 lines" }
        }
    }
}
