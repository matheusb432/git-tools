use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const SETTINGS: GuidedTour = GuidedTour::new("settings", settings_steps);
pub(crate) const SETTINGS_NAVIGATION: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-navigation");
pub(crate) const SETTINGS_CONTENT: GuidedTourAnchor = GuidedTourAnchor::new("settings-content");
pub(crate) const SETTINGS_SOURCE: GuidedTourAnchor = GuidedTourAnchor::new("settings-source");
pub(crate) const SETTINGS_BACK: GuidedTourAnchor = GuidedTourAnchor::new("settings-back");

fn settings_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            SETTINGS_NAVIGATION,
            t!(language, "tour-settings-1-title"),
            t!(language, "tour-settings-1-body"),
        ),
        GuidedTourStep::new(
            SETTINGS_CONTENT,
            t!(language, "tour-settings-2-title"),
            t!(language, "tour-settings-2-body"),
        ),
        GuidedTourStep::new(
            SETTINGS_SOURCE,
            t!(language, "tour-settings-3-title"),
            t!(language, "tour-settings-3-body"),
        ),
        GuidedTourStep::new(
            SETTINGS_BACK,
            t!(language, "tour-settings-4-title"),
            t!(language, "tour-settings-4-body"),
        ),
    ]
}
