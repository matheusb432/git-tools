use dioxus::prelude::Element;

mod alert_dialog;
mod badge;
mod button;
mod empty_notice;
mod icon_popover;
mod skeleton;
mod text_input;
mod toast;

#[derive(Clone, Copy)]
pub(super) struct Story {
    pub(super) slug: &'static str,
    pub(super) title: &'static str,
    pub(super) summary: &'static str,
    pub(super) preview: StoryPreview,
    pub(super) variants: &'static [StoryVariant],
}

impl PartialEq for Story {
    fn eq(&self, other: &Self) -> bool {
        self.slug == other.slug
            && self.title == other.title
            && self.summary == other.summary
            && self.preview == other.preview
            && self.variants == other.variants
    }
}

#[derive(Clone, Copy)]
pub(super) struct StoryPreview {
    pub(super) render: fn(StoryContext) -> Element,
}

impl PartialEq for StoryPreview {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.render, other.render)
    }
}

#[derive(Clone, Copy)]
pub(super) struct StoryVariant {
    pub(super) slug: &'static str,
    pub(super) title: &'static str,
    pub(super) summary: &'static str,
    pub(super) render: fn(StoryContext) -> Element,
}

impl PartialEq for StoryVariant {
    fn eq(&self, other: &Self) -> bool {
        self.slug == other.slug
            && self.title == other.title
            && self.summary == other.summary
            && std::ptr::fn_addr_eq(self.render, other.render)
    }
}

#[derive(Clone, Copy)]
pub(super) struct StoryContext {
    pub(super) reset_generation: u64,
}

const STORIES: &[Story] = &[
    Story {
        slug: "button",
        title: "Button",
        summary: "Button component.",
        preview: StoryPreview {
            render: button::preview,
        },
        variants: button::VARIANTS,
    },
    Story {
        slug: "badge",
        title: "Badge",
        summary: "Badge component.",
        preview: StoryPreview {
            render: badge::preview,
        },
        variants: badge::VARIANTS,
    },
    Story {
        slug: "text-input",
        title: "Text input",
        summary: "Text input component.",
        preview: StoryPreview {
            render: text_input::preview,
        },
        variants: text_input::VARIANTS,
    },
    Story {
        slug: "empty-notice",
        title: "Empty notice",
        summary: "Empty notice component.",
        preview: StoryPreview {
            render: empty_notice::preview,
        },
        variants: empty_notice::VARIANTS,
    },
    Story {
        slug: "skeleton",
        title: "Skeleton",
        summary: "Loading placeholder component.",
        preview: StoryPreview {
            render: skeleton::preview,
        },
        variants: skeleton::VARIANTS,
    },
    Story {
        slug: "icon-popover",
        title: "Icon popover",
        summary: "Icon-triggered popover component.",
        preview: StoryPreview {
            render: icon_popover::preview,
        },
        variants: icon_popover::VARIANTS,
    },
    Story {
        slug: "alert-dialog",
        title: "Alert dialog",
        summary: "Confirmation dialog component.",
        preview: StoryPreview {
            render: alert_dialog::preview,
        },
        variants: alert_dialog::VARIANTS,
    },
    Story {
        slug: "toast",
        title: "Toast",
        summary: "Toast notification component.",
        preview: StoryPreview {
            render: toast::preview,
        },
        variants: toast::VARIANTS,
    },
];

pub(super) const fn all() -> &'static [Story] {
    STORIES
}

pub(super) fn find(
    story_slug: &str,
    variant_slug: &str,
) -> Option<(&'static Story, &'static StoryVariant)> {
    let story = STORIES.iter().find(|story| story.slug == story_slug)?;
    let variant = story
        .variants
        .iter()
        .find(|variant| variant.slug == variant_slug)?;
    Some((story, variant))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{STORIES, find};

    #[test]
    fn catalog_has_unique_stable_paths() {
        let mut paths = BTreeSet::new();
        for story in STORIES {
            assert!(!story.slug.is_empty(), "story slug must not be empty");
            assert!(!story.title.is_empty(), "story title must not be empty");
            assert!(
                !story.variants.is_empty(),
                "{} must have at least one variant",
                story.slug
            );
        }

        let variants = STORIES.iter().flat_map(|story| {
            story
                .variants
                .iter()
                .map(move |variant| (story.slug, variant))
        });
        for (story_slug, variant) in variants {
            assert!(!variant.slug.is_empty(), "variant slug must not be empty");
            assert!(!variant.title.is_empty(), "variant title must not be empty");
            assert!(
                paths.insert((story_slug, variant.slug)),
                "duplicate story path: {story_slug}/{}",
                variant.slug
            );
        }
    }

    #[test]
    fn lookup_resolves_only_cataloged_paths() {
        assert!(find("button", "interactive").is_some());
        assert!(find("button", "missing").is_none());
        assert!(find("missing", "interactive").is_none());
    }
}
