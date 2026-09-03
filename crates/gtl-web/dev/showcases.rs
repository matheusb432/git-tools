mod alert_dialog;
mod badge;
mod button;
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

    use dx_preview::{find, showcases};

    #[test]
    fn catalog_has_unique_stable_paths() {
        let showcases = showcases().unwrap();
        let mut showcase_ids = BTreeSet::new();

        for showcase in showcases {
            assert!(!showcase.id().is_empty(), "showcase ID must not be empty");
            assert!(
                !showcase.name().is_empty(),
                "showcase name must not be empty"
            );
            assert!(
                showcase_ids.insert(showcase.id()),
                "duplicate showcase ID: {}",
                showcase.id()
            );
            assert!(
                !showcase.previews().is_empty(),
                "{} must have at least one preview",
                showcase.id()
            );
            assert!(
                showcase.thumbnail().is_some(),
                "{} must define a compact catalog thumbnail",
                showcase.id()
            );
        }

        let previews = showcases.iter().flat_map(|showcase| {
            showcase
                .previews()
                .iter()
                .copied()
                .map(move |preview| (showcase.id(), preview))
        });
        let mut paths = BTreeSet::new();
        for (showcase_id, preview) in previews {
            assert!(!preview.id().is_empty(), "preview ID must not be empty");
            assert!(!preview.name().is_empty(), "preview name must not be empty");
            assert!(
                paths.insert((showcase_id, preview.id())),
                "duplicate showcase path: {showcase_id}/{}",
                preview.id()
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
            find("viewer-settings-redesign", "settings-form")
                .unwrap()
                .is_some()
        );
        assert!(find("button", "missing").unwrap().is_none());
        assert!(find("missing", "interactive").unwrap().is_none());
    }
}
