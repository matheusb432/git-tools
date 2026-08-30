mod alert_dialog;
mod badge;
mod button;
mod empty_notice;
mod icon_popover;
mod skeleton;
mod text_input;
mod toast;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use dx_book::{find, stories};

    #[test]
    fn catalog_has_unique_stable_paths() {
        let stories = stories().unwrap();
        let mut story_ids = BTreeSet::new();

        for story in stories {
            assert!(!story.id().is_empty(), "story ID must not be empty");
            assert!(!story.name().is_empty(), "story name must not be empty");
            assert!(
                story_ids.insert(story.id()),
                "duplicate story ID: {}",
                story.id()
            );
            assert!(
                !story.variants().is_empty(),
                "{} must have at least one variant",
                story.id()
            );
            assert!(
                story.preview().is_some(),
                "{} must define a compact catalog preview",
                story.id()
            );
        }

        let variants = stories.iter().flat_map(|story| {
            story
                .variants()
                .iter()
                .copied()
                .map(move |variant| (story.id(), variant))
        });
        let mut paths = BTreeSet::new();
        for (story_id, variant) in variants {
            assert!(!variant.id().is_empty(), "variant ID must not be empty");
            assert!(!variant.name().is_empty(), "variant name must not be empty");
            assert!(
                paths.insert((story_id, variant.id())),
                "duplicate story path: {story_id}/{}",
                variant.id()
            );
        }
    }

    #[test]
    fn lookup_resolves_only_cataloged_paths() {
        assert!(find("button", "interactive").unwrap().is_some());
        assert!(find("button", "missing").unwrap().is_none());
        assert!(find("missing", "interactive").unwrap().is_none());
    }
}
