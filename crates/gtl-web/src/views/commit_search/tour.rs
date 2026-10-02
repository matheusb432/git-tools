use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const FINDER: GuidedTour = GuidedTour::new("commit-finder", finder_steps);
pub(crate) const FINDER_QUERY: GuidedTourAnchor = GuidedTourAnchor::new("commit-finder-query");
pub(crate) const FINDER_TIME: GuidedTourAnchor = GuidedTourAnchor::new("commit-finder-time");
pub(crate) const FINDER_RESULTS: GuidedTourAnchor = GuidedTourAnchor::new("commit-finder-results");

fn finder_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            FINDER_QUERY,
            t!(language, "tour-finder-1-title"),
            t!(language, "tour-finder-1-body"),
        ),
        GuidedTourStep::new(
            FINDER_TIME,
            t!(language, "tour-finder-2-title"),
            t!(language, "tour-finder-2-body"),
        ),
        GuidedTourStep::new(
            FINDER_RESULTS,
            t!(language, "tour-finder-3-title"),
            t!(language, "tour-finder-3-body"),
        ),
    ]
}
