mod alert_dialog;
mod badge;
mod button;
mod data_table;
mod empty_notice;
mod icon_popover;
mod loading_spinner;
mod page_notice;
mod skeleton;
mod text_input;
mod toast;
mod viewer_settings_redesign;
mod viewer_tab_overflow_menu;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use dx_story::{find, story_sets};

    #[test]
    fn catalog_has_unique_stable_paths() {
        let story_sets = story_sets().unwrap();
        let mut story_set_ids = BTreeSet::new();

        for story_set in story_sets {
            assert!(!story_set.id().is_empty(), "story set ID must not be empty");
            assert!(
                !story_set.name().is_empty(),
                "story set name must not be empty"
            );
            assert!(
                story_set_ids.insert(story_set.id()),
                "duplicate story set ID: {}",
                story_set.id()
            );
            assert!(
                !story_set.stories().is_empty(),
                "{} must have at least one story",
                story_set.id()
            );
            assert!(
                story_set.thumbnail().is_some(),
                "{} must define a compact catalog thumbnail",
                story_set.id()
            );
        }

        let stories = story_sets.iter().flat_map(|story_set| {
            story_set
                .stories()
                .iter()
                .copied()
                .map(move |story| (story_set.id(), story))
        });
        let mut paths = BTreeSet::new();
        for (story_set_id, story) in stories {
            assert!(!story.id().is_empty(), "story ID must not be empty");
            assert!(!story.name().is_empty(), "story name must not be empty");
            assert!(
                paths.insert((story_set_id, story.id())),
                "duplicate story path: {story_set_id}/{}",
                story.id()
            );
        }
    }

    #[test]
    fn lookup_resolves_only_cataloged_paths() {
        assert!(find("button", "interactive").unwrap().is_some());
        assert!(
            find("viewer-tab-overflow-menu", "interactive")
                .unwrap()
                .is_some()
        );
        assert!(
            find("viewer-tab-overflow-menu", "narrow-rail")
                .unwrap()
                .is_some()
        );
        assert!(
            find("viewer-settings-redesign", "desktop-viewer")
                .unwrap()
                .is_some()
        );
        assert!(
            find("viewer-settings-redesign", "mobile-viewer")
                .unwrap()
                .is_some()
        );
        assert!(
            find("viewer-settings-redesign", "search-all-files")
                .unwrap()
                .is_some()
        );
        assert!(
            find("viewer-settings-redesign", "settings-form")
                .unwrap()
                .is_some()
        );
        assert!(find("button", "missing").unwrap().is_none());
        assert!(find("missing", "interactive").unwrap().is_none());
    }
}
