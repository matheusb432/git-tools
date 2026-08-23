//! Immutable, validated user settings used by application operations.

pub mod setting_key;

use std::collections::BTreeSet;

pub use setting_key::*;

use crate::{
    diffs::DiffExclusions,
    paths::ProjectName,
    viewer::{RenderOptions, Theme},
};

/// Exact, case-sensitive sample_project project names omitted from `push --all` before Git inspection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushAllExclusions(BTreeSet<ProjectName>);

impl PushAllExclusions {
    pub fn new(projects: impl IntoIterator<Item = ProjectName>) -> Self {
        Self(projects.into_iter().collect())
    }

    pub fn contains(&self, project: &ProjectName) -> bool {
        self.0.contains(project)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// TODO: remove Clone once a store-owned smart pointer is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSettings {
    theme: Option<Theme>,
    viewer_render_options: RenderOptions,
    push_confirmation_required: bool,
    diff_exclusions: DiffExclusions,
    push_all_exclusions: PushAllExclusions,
}

impl UserSettings {
    pub const PUSH_CONFIRMATION_REQUIRED_DEFAULT: bool = true;

    /// Constructs a complete settings snapshot from validated values.
    pub fn new(
        theme: Option<Theme>,
        viewer_render_options: RenderOptions,
        push_confirmation_required: bool,
        diff_exclusions: DiffExclusions,
        push_all_exclusions: PushAllExclusions,
    ) -> Self {
        Self {
            theme,
            viewer_render_options,
            push_confirmation_required,
            diff_exclusions,
            push_all_exclusions,
        }
    }

    /// Returns the selected theme, when one is configured.
    pub const fn theme(&self) -> Option<Theme> {
        self.theme
    }

    /// Returns the validated viewer layout and density.
    pub const fn viewer_render_options(&self) -> RenderOptions {
        self.viewer_render_options
    }

    /// Returns whether a plain current-repository push requires confirmation.
    pub const fn push_confirmation_required(&self) -> bool {
        self.push_confirmation_required
    }

    /// Returns the complete validated project and default exclusion map.
    pub const fn diff_exclusions(&self) -> &DiffExclusions {
        &self.diff_exclusions
    }

    /// Returns the configured project names omitted from managed push fan-out.
    pub const fn push_all_exclusions(&self) -> &PushAllExclusions {
        &self.push_all_exclusions
    }
}

#[cfg(test)]
mod tests {
    use super::{PushAllExclusions, UserSettings};
    use crate::{
        diffs::DiffExclusions,
        paths::{ProjectName, RepositoryRelativePath},
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    fn project(value: &str) -> ProjectName {
        ProjectName::try_new(value.to_owned()).expect("project name")
    }

    fn path(value: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(value.into()).expect("repository-relative path")
    }

    #[test]
    fn accessors_expose_the_complete_immutable_snapshot() {
        let settings = UserSettings::new(
            Some(Theme::Hearth),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            false,
            DiffExclusions::new([(project("git-tools"), vec!["md", "lock"])], None),
            PushAllExclusions::new([project("sample_project")]),
        );

        assert_eq!(settings.theme(), Some(Theme::Hearth));
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert!(!settings.push_confirmation_required());
        assert!(settings.push_all_exclusions().contains(&project("sample_project")));
        assert!(
            !settings
                .push_all_exclusions()
                .contains(&project("git-tools"))
        );
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(&project("git-tools"))
                .matches(&path("README.md"))
        );
        assert!(
            !settings
                .diff_exclusions()
                .for_project_or_default(&project("git-tools"))
                .matches(&path("src/main.rs"))
        );
    }

    #[test]
    fn constructor_preserves_an_absent_theme_and_validated_defaults() {
        let settings = UserSettings::new(
            None,
            RenderOptions::DEFAULT,
            true,
            DiffExclusions::default(),
            PushAllExclusions::default(),
        );

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(settings.diff_exclusions().is_empty());
        assert!(settings.push_all_exclusions().is_empty());
    }
}
