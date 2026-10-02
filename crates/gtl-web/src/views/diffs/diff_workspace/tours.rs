use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(crate) const WORKSPACE: GuidedTour = GuidedTour::new("diff-workspace", workspace_steps);
pub(crate) const WORKSPACE_PROJECTS: GuidedTourAnchor =
    GuidedTourAnchor::new("diff-workspace-projects");
pub(crate) const WORKSPACE_TABS: GuidedTourAnchor = GuidedTourAnchor::new("diff-workspace-tabs");
pub(crate) const WORKSPACE_DETAILS: GuidedTourAnchor =
    GuidedTourAnchor::new("diff-workspace-details");
pub(crate) const WORKSPACE_SIDEBARS: GuidedTourAnchor =
    GuidedTourAnchor::new("diff-workspace-sidebars");
pub(crate) const WORKSPACE_REVIEW: GuidedTourAnchor =
    GuidedTourAnchor::new("diff-workspace-review");
pub(crate) const FILES: GuidedTour = GuidedTour::new("diff-files", files_steps);
pub(crate) const FILES_LIST: GuidedTourAnchor = GuidedTourAnchor::new("diff-files-list");
pub(crate) const FILES_SORT: GuidedTourAnchor = GuidedTourAnchor::new("diff-files-sort");
pub(crate) const FILES_FILTER: GuidedTourAnchor = GuidedTourAnchor::new("diff-files-filter");
pub(crate) const FILES_TOTALS: GuidedTourAnchor = GuidedTourAnchor::new("diff-files-totals");
pub(crate) const COMMITS: GuidedTour = GuidedTour::new("diff-commits", commits_steps);
pub(crate) const COMMITS_LIST: GuidedTourAnchor = GuidedTourAnchor::new("diff-commits-list");
pub(crate) const COMMITS_SEARCH: GuidedTourAnchor = GuidedTourAnchor::new("diff-commits-search");
pub(crate) const COMMITS_ACTIONS: GuidedTourAnchor = GuidedTourAnchor::new("diff-commits-actions");

fn workspace_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            WORKSPACE_PROJECTS,
            t!(language, "tour-workspace-1-title"),
            t!(language, "tour-workspace-1-body"),
        ),
        GuidedTourStep::new(
            WORKSPACE_TABS,
            t!(language, "tour-workspace-2-title"),
            t!(language, "tour-workspace-2-body"),
        ),
        GuidedTourStep::new(
            WORKSPACE_DETAILS,
            t!(language, "tour-workspace-3-title"),
            t!(language, "tour-workspace-3-body"),
        ),
        GuidedTourStep::new(
            WORKSPACE_SIDEBARS,
            t!(language, "tour-workspace-4-title"),
            t!(language, "tour-workspace-4-body"),
        ),
        GuidedTourStep::new(
            WORKSPACE_REVIEW,
            t!(language, "tour-workspace-5-title"),
            t!(language, "tour-workspace-5-body"),
        ),
    ]
}

fn files_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            FILES_LIST,
            t!(language, "tour-files-1-title"),
            t!(language, "tour-files-1-body"),
        ),
        GuidedTourStep::new(
            FILES_SORT,
            t!(language, "tour-files-2-title"),
            t!(language, "tour-files-2-body"),
        ),
        GuidedTourStep::new(
            FILES_FILTER,
            t!(language, "tour-files-3-title"),
            t!(language, "tour-files-3-body"),
        ),
        GuidedTourStep::new(
            FILES_TOTALS,
            t!(language, "tour-files-4-title"),
            t!(language, "tour-files-4-body"),
        ),
    ]
}

fn commits_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            COMMITS_LIST,
            t!(language, "tour-commits-1-title"),
            t!(language, "tour-commits-1-body"),
        ),
        GuidedTourStep::new(
            COMMITS_SEARCH,
            t!(language, "tour-commits-2-title"),
            t!(language, "tour-commits-2-body"),
        ),
        GuidedTourStep::new(
            COMMITS_SEARCH,
            t!(language, "tour-commits-3-title"),
            t!(language, "tour-commits-3-body"),
        ),
        GuidedTourStep::new(
            COMMITS_ACTIONS,
            t!(language, "tour-commits-4-title"),
            t!(language, "tour-commits-4-body"),
        ),
    ]
}
