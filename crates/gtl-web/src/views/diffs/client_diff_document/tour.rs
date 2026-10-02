use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const READING: GuidedTour = GuidedTour::new("diff-reading", reading_steps);
pub(crate) const READING_HEADER: GuidedTourAnchor = GuidedTourAnchor::new("diff-reading-header");
pub(crate) const READING_DOCUMENT: GuidedTourAnchor =
    GuidedTourAnchor::new("diff-reading-document");
pub(crate) const READING_ACTIONS: GuidedTourAnchor = GuidedTourAnchor::new("diff-reading-actions");

fn reading_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            READING_HEADER,
            t!(language, "tour-reading-1-title"),
            t!(language, "tour-reading-1-body"),
        ),
        GuidedTourStep::new(
            READING_DOCUMENT,
            t!(language, "tour-reading-2-title"),
            t!(language, "tour-reading-2-body"),
        ),
        GuidedTourStep::new(
            READING_ACTIONS,
            t!(language, "tour-reading-3-title"),
            t!(language, "tour-reading-3-body"),
        ),
        GuidedTourStep::new(
            READING_DOCUMENT,
            t!(language, "tour-reading-4-title"),
            t!(language, "tour-reading-4-body"),
        ),
    ]
}
