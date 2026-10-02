use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const HISTORY: GuidedTour = GuidedTour::new("snapshot-history", history_steps);
pub(crate) const HISTORY_FILTER: GuidedTourAnchor =
    GuidedTourAnchor::new("snapshot-history-filter");
pub(crate) const HISTORY_LIST: GuidedTourAnchor = GuidedTourAnchor::new("snapshot-history-list");
pub(crate) const HISTORY_ACTIONS: GuidedTourAnchor =
    GuidedTourAnchor::new("snapshot-history-actions");

fn history_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            HISTORY_FILTER,
            t!(language, "tour-history-1-title"),
            t!(language, "tour-history-1-body"),
        ),
        GuidedTourStep::new(
            HISTORY_LIST,
            t!(language, "tour-history-2-title"),
            t!(language, "tour-history-2-body"),
        ),
        GuidedTourStep::new(
            HISTORY_ACTIONS,
            t!(language, "tour-history-3-title"),
            t!(language, "tour-history-3-body"),
        ),
    ]
}
