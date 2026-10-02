use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::t,
    ui::guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourStep},
};

pub(super) const fn for_section(section: super::SettingsSection) -> GuidedTour {
    match section {
        super::SettingsSection::Appearance => APPEARANCE,
        super::SettingsSection::Locale => LOCALE,
        super::SettingsSection::Snapshots => SNAPSHOTS,
        super::SettingsSection::Git => GIT,
        super::SettingsSection::Keybindings => KEYBINDINGS,
    }
}

pub(crate) const APPEARANCE: GuidedTour = GuidedTour::new("settings-appearance", appearance_steps);
pub(crate) const APPEARANCE_THEME: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-appearance-theme");
pub(crate) const APPEARANCE_SCALE: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-appearance-scale");
pub(crate) const APPEARANCE_MOTION: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-appearance-motion");
pub(crate) const LOCALE: GuidedTour = GuidedTour::new("settings-locale", locale_steps);
pub(crate) const LOCALE_LANGUAGE: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-locale-language");
pub(crate) const LOCALE_DATES: GuidedTourAnchor = GuidedTourAnchor::new("settings-locale-dates");
pub(crate) const SNAPSHOTS: GuidedTour = GuidedTour::new("settings-snapshots", snapshots_steps);
pub(crate) const SNAPSHOTS_LAYOUT: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-snapshots-layout");
pub(crate) const SNAPSHOTS_DENSITY: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-snapshots-density");
pub(crate) const SNAPSHOTS_WRAP: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-snapshots-wrap");
pub(crate) const SNAPSHOTS_COPY: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-snapshots-copy");
pub(crate) const GIT: GuidedTour = GuidedTour::new("settings-git", git_steps);
pub(crate) const GIT_FOCUS: GuidedTourAnchor = GuidedTourAnchor::new("settings-git-focus");
pub(crate) const GIT_CLI_PUSH: GuidedTourAnchor = GuidedTourAnchor::new("settings-git-cli-push");
pub(crate) const GIT_VIEWER_PUSH: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-git-viewer-push");
pub(crate) const KEYBINDINGS: GuidedTour =
    GuidedTour::new("settings-keybindings", keybindings_steps);
pub(crate) const KEYBINDINGS_SEARCH: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-keybindings-search");
pub(crate) const KEYBINDINGS_RECORD: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-keybindings-record");
pub(crate) const KEYBINDINGS_MODIFIED: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-keybindings-modified");
pub(crate) const KEYBINDINGS_TABLE: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-keybindings-table");
pub(crate) const KEYBINDINGS_RESET: GuidedTourAnchor =
    GuidedTourAnchor::new("settings-keybindings-reset");

fn appearance_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            APPEARANCE_THEME,
            t!(language, "tour-appearance-1-title"),
            t!(language, "tour-appearance-1-body"),
        ),
        GuidedTourStep::new(
            APPEARANCE_SCALE,
            t!(language, "tour-appearance-2-title"),
            t!(language, "tour-appearance-2-body"),
        ),
        GuidedTourStep::new(
            APPEARANCE_MOTION,
            t!(language, "tour-appearance-3-title"),
            t!(language, "tour-appearance-3-body"),
        ),
    ]
}

fn locale_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            LOCALE_LANGUAGE,
            t!(language, "tour-locale-1-title"),
            t!(language, "tour-locale-1-body"),
        ),
        GuidedTourStep::new(
            LOCALE_DATES,
            t!(language, "tour-locale-2-title"),
            t!(language, "tour-locale-2-body"),
        ),
    ]
}

fn snapshots_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            SNAPSHOTS_LAYOUT,
            t!(language, "tour-snapshots-1-title"),
            t!(language, "tour-snapshots-1-body"),
        ),
        GuidedTourStep::new(
            SNAPSHOTS_DENSITY,
            t!(language, "tour-snapshots-2-title"),
            t!(language, "tour-snapshots-2-body"),
        ),
        GuidedTourStep::new(
            SNAPSHOTS_WRAP,
            t!(language, "tour-snapshots-3-title"),
            t!(language, "tour-snapshots-3-body"),
        ),
        GuidedTourStep::new(
            SNAPSHOTS_COPY,
            t!(language, "tour-snapshots-4-title"),
            t!(language, "tour-snapshots-4-body"),
        ),
    ]
}

fn git_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            GIT_FOCUS,
            t!(language, "tour-git-1-title"),
            t!(language, "tour-git-1-body"),
        ),
        GuidedTourStep::new(
            GIT_CLI_PUSH,
            t!(language, "tour-git-2-title"),
            t!(language, "tour-git-2-body"),
        ),
        GuidedTourStep::new(
            GIT_VIEWER_PUSH,
            t!(language, "tour-git-3-title"),
            t!(language, "tour-git-3-body"),
        ),
    ]
}

fn keybindings_steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    vec![
        GuidedTourStep::new(
            KEYBINDINGS_SEARCH,
            t!(language, "tour-keybindings-1-title"),
            t!(language, "tour-keybindings-1-body"),
        ),
        GuidedTourStep::new(
            KEYBINDINGS_RECORD,
            t!(language, "tour-keybindings-2-title"),
            t!(language, "tour-keybindings-2-body"),
        ),
        GuidedTourStep::new(
            KEYBINDINGS_MODIFIED,
            t!(language, "tour-keybindings-3-title"),
            t!(language, "tour-keybindings-3-body"),
        ),
        GuidedTourStep::new(
            KEYBINDINGS_TABLE,
            t!(language, "tour-keybindings-4-title"),
            t!(language, "tour-keybindings-4-body"),
        ),
        GuidedTourStep::new(
            KEYBINDINGS_RESET,
            t!(language, "tour-keybindings-5-title"),
            t!(language, "tour-keybindings-5-body"),
        ),
    ]
}
