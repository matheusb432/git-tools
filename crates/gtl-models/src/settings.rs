//! Immutable, validated user settings used by application operations.

pub mod setting_key;

pub use setting_key::*;

use crate::{
    diffs::DiffExclusions,
    viewer::{RenderOptions, Theme},
};

// TODO: remove Clone once a store-owned smart pointer is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSettings {
    theme: Option<Theme>,
    viewer_render_options: RenderOptions,
    push_confirmation_required: bool,
    diff_exclusions: DiffExclusions,
}

impl UserSettings {
    pub const PUSH_CONFIRMATION_REQUIRED_DEFAULT: bool = true;

    /// Constructs a complete settings snapshot from validated values.
    pub fn new(
        theme: Option<Theme>,
        viewer_render_options: RenderOptions,
        push_confirmation_required: bool,
        diff_exclusions: DiffExclusions,
    ) -> Self {
        Self {
            theme,
            viewer_render_options,
            push_confirmation_required,
            diff_exclusions,
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
}

#[cfg(test)]
mod tests {
    use super::UserSettings;
    use crate::{
        diffs::DiffExclusions,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    #[test]
    fn accessors_expose_the_complete_immutable_snapshot() {
        let settings = UserSettings::new(
            Some(Theme::Hearth),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            false,
            DiffExclusions::new([("git-tools".to_owned(), vec!["md", "lock"])], None),
        );

        assert_eq!(settings.theme(), Some(Theme::Hearth));
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert!(!settings.push_confirmation_required());
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default("git-tools")
                .matches("README.md")
        );
        assert!(
            !settings
                .diff_exclusions()
                .for_project_or_default("git-tools")
                .matches("src/main.rs")
        );
    }

    #[test]
    fn constructor_preserves_an_absent_theme_and_validated_defaults() {
        let settings = UserSettings::new(
            None,
            RenderOptions::DEFAULT,
            true,
            DiffExclusions::default(),
        );

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(settings.diff_exclusions().is_empty());
    }
}
