use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const PROJECTS: GuidedTour = GuidedTour::new("projects", projects_steps);
pub(crate) const PROJECTS_ADD: GuidedTourAnchor = GuidedTourAnchor::new("projects-add");
pub(crate) const PROJECTS_TABLE: GuidedTourAnchor = GuidedTourAnchor::new("projects-table");
pub(crate) const PROJECTS_FILTER: GuidedTourAnchor = GuidedTourAnchor::new("projects-filter");
pub(crate) const PROJECTS_ACTIONS: GuidedTourAnchor = GuidedTourAnchor::new("projects-actions");
pub(crate) const PROJECTS_BATCH: GuidedTourAnchor = GuidedTourAnchor::new("projects-batch");
pub(crate) const PROJECTS_HISTORY: GuidedTourAnchor = GuidedTourAnchor::new("projects-history");
pub(crate) const COMPARISON: GuidedTour = GuidedTour::new("project-comparison", comparison_steps);
pub(crate) const COMPARISON_BRANCH: GuidedTourAnchor =
    GuidedTourAnchor::new("project-comparison-branch");
pub(crate) const COMPARISON_PUSH: GuidedTourAnchor =
    GuidedTourAnchor::new("project-comparison-push");
pub(crate) const COMPARISON_HISTORY: GuidedTourAnchor =
    GuidedTourAnchor::new("project-comparison-history");
pub(crate) const COMPARISON_SAVE: GuidedTourAnchor =
    GuidedTourAnchor::new("project-comparison-save");
pub(crate) const IMPORT: GuidedTour = GuidedTour::new("project-import", import_steps);
pub(crate) const IMPORT_SCAN: GuidedTourAnchor = GuidedTourAnchor::new("project-import-scan");
pub(crate) const IMPORT_ROWS: GuidedTourAnchor = GuidedTourAnchor::new("project-import-rows");
pub(crate) const IMPORT_SUBMIT: GuidedTourAnchor = GuidedTourAnchor::new("project-import-submit");

fn projects_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            PROJECTS_ADD,
            t!(language, "tour-projects-1-title"),
            t!(language, "tour-projects-1-body"),
        ),
        GuidedTourStep::new(
            PROJECTS_TABLE,
            t!(language, "tour-projects-2-title"),
            t!(language, "tour-projects-2-body"),
        ),
        GuidedTourStep::new(
            PROJECTS_FILTER,
            t!(language, "tour-projects-3-title"),
            t!(language, "tour-projects-3-body"),
        ),
        GuidedTourStep::new(
            PROJECTS_ACTIONS,
            t!(language, "tour-projects-4-title"),
            t!(language, "tour-projects-4-body"),
        ),
        GuidedTourStep::new(
            PROJECTS_BATCH,
            t!(language, "tour-projects-5-title"),
            t!(language, "tour-projects-5-body"),
        ),
        GuidedTourStep::new(
            PROJECTS_HISTORY,
            t!(language, "tour-projects-6-title"),
            t!(language, "tour-projects-6-body"),
        ),
    ]
}

fn comparison_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            COMPARISON_BRANCH,
            t!(language, "tour-comparison-1-title"),
            t!(language, "tour-comparison-1-body"),
        ),
        GuidedTourStep::new(
            COMPARISON_PUSH,
            t!(language, "tour-comparison-2-title"),
            t!(language, "tour-comparison-2-body"),
        ),
        GuidedTourStep::new(
            COMPARISON_HISTORY,
            t!(language, "tour-comparison-3-title"),
            t!(language, "tour-comparison-3-body"),
        ),
        GuidedTourStep::new(
            COMPARISON_SAVE,
            t!(language, "tour-comparison-4-title"),
            t!(language, "tour-comparison-4-body"),
        ),
    ]
}

fn import_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            IMPORT_SCAN,
            t!(language, "tour-import-1-title"),
            t!(language, "tour-import-1-body"),
        ),
        GuidedTourStep::new(
            IMPORT_ROWS,
            t!(language, "tour-import-2-title"),
            t!(language, "tour-import-2-body"),
        ),
        GuidedTourStep::new(
            IMPORT_SUBMIT,
            t!(language, "tour-import-3-title"),
            t!(language, "tour-import-3-body"),
        ),
    ]
}
