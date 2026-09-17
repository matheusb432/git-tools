//! Immutable, validated user settings used by application operations.

pub mod setting_key;

use std::collections::BTreeSet;

pub use setting_key::*;

use crate::{
    diffs::DiffExclusions,
    paths::ProjectName,
    tags::TagPatternSettings,
    viewer::{RenderOptions, Theme, ViewerKeybindings},
};

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProjectsViewMode {
    #[default]
    Grid,
    Table,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProjectsSort {
    #[default]
    Changes,
    Name,
    Branch,
    ChangesAscending,
    NameDescending,
    BranchDescending,
}

#[nutype::nutype(
    validate(predicate = |value| matches!(value, 10 | 15 | 30)),
    default = 15,
    derive(Debug, Clone, Copy, Default, PartialEq, Eq, Display, Serialize, Deserialize)
)]
pub struct ProjectsPageSize(u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectsPreferences {
    pub view: ProjectsViewMode,
    pub page_size: ProjectsPageSize,
    pub sort: ProjectsSort,
}

/// SHA-256 identity of one exact serialized user-settings document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserSettingsRevision([u8; 32]);

impl UserSettingsRevision {
    /// Creates a revision from the settings document digest.
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the underlying SHA-256 digest.
    #[must_use]
    pub const fn into_digest(self) -> [u8; 32] {
        self.0
    }
}

impl std::fmt::Display for UserSettingsRevision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for UserSettingsRevision {
    type Err = UserSettingsRevisionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(UserSettingsRevisionError);
        }
        let mut digest = [0; 32];
        let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(UserSettingsRevisionError);
        }
        for (output, pair) in digest.iter_mut().zip(pairs) {
            let high = lowercase_hexadecimal_nibble(pair[0]).ok_or(UserSettingsRevisionError)?;
            let low = lowercase_hexadecimal_nibble(pair[1]).ok_or(UserSettingsRevisionError)?;
            *output = (high << 4) | low;
        }
        Ok(Self(digest))
    }
}

impl TryFrom<String> for UserSettingsRevision {
    type Error = UserSettingsRevisionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl serde::Serialize for UserSettingsRevision {
    fn serialize<SerializerType>(
        &self,
        serializer: SerializerType,
    ) -> Result<SerializerType::Ok, SerializerType::Error>
    where
        SerializerType: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for UserSettingsRevision {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        String::deserialize(deserializer)?
            .parse()
            .map_err(DeserializerType::Error::custom)
    }
}

const fn lowercase_hexadecimal_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Reports a malformed serialized user-settings revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("user-settings revision must contain exactly 64 lowercase hexadecimal characters")]
pub struct UserSettingsRevisionError;

/// Exact, case-sensitive project names omitted from `project push --all` before Git inspection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushAllExclusions(BTreeSet<ProjectName>);

impl PushAllExclusions {
    pub fn new(projects: impl IntoIterator<Item = ProjectName>) -> Self {
        Self(projects.into_iter().collect())
    }

    #[must_use]
    pub fn contains(&self, project: &ProjectName) -> bool {
        self.0.contains(project)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates excluded project names in stable order.
    #[must_use]
    pub fn projects(&self) -> impl ExactSizeIterator<Item = &ProjectName> {
        self.0.iter()
    }
}

// TODO: remove Clone once a store-owned smart pointer is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSettings {
    focus_window_on_diff: bool,
    theme: Option<Theme>,
    viewer_render_options: RenderOptions,
    viewer_keybindings: ViewerKeybindings,
    sidebar_visibility: crate::viewer::ViewerSidebarVisibility,
    push_confirmation_required: bool,
    diff_exclusions: DiffExclusions,
    push_all_exclusions: PushAllExclusions,
    tag_patterns: TagPatternSettings,
}

impl UserSettings {
    pub const PUSH_CONFIRMATION_REQUIRED_DEFAULT: bool = true;

    /// Constructs a complete settings snapshot from validated values.
    #[must_use]
    pub fn new(
        theme: Option<Theme>,
        viewer_render_options: RenderOptions,
        viewer_keybindings: ViewerKeybindings,
        push_confirmation_required: bool,
        diff_exclusions: DiffExclusions,
        push_all_exclusions: PushAllExclusions,
    ) -> Self {
        Self {
            focus_window_on_diff: true,
            theme,
            viewer_render_options,
            viewer_keybindings,
            sidebar_visibility: crate::viewer::ViewerSidebarVisibility::default(),
            push_confirmation_required,
            diff_exclusions,
            push_all_exclusions,
            tag_patterns: TagPatternSettings::default(),
        }
    }

    #[must_use]
    pub fn with_diff_exclusions(self, diff_exclusions: DiffExclusions) -> Self {
        Self {
            diff_exclusions,
            ..self
        }
    }

    #[must_use]
    pub fn with_focus_window_on_diff(self, focus_window_on_diff: bool) -> Self {
        Self {
            focus_window_on_diff,
            ..self
        }
    }

    #[must_use]
    pub const fn focus_window_on_diff(&self) -> bool {
        self.focus_window_on_diff
    }

    #[must_use]
    pub fn with_sidebar_visibility(
        self,
        sidebar_visibility: crate::viewer::ViewerSidebarVisibility,
    ) -> Self {
        Self {
            sidebar_visibility,
            ..self
        }
    }

    #[must_use]
    pub const fn sidebar_visibility(&self) -> crate::viewer::ViewerSidebarVisibility {
        self.sidebar_visibility
    }

    #[must_use]
    pub fn with_tag_patterns(self, tag_patterns: TagPatternSettings) -> Self {
        Self {
            tag_patterns,
            ..self
        }
    }

    /// Returns the selected theme, when one is configured.
    #[must_use]
    pub const fn theme(&self) -> Option<Theme> {
        self.theme
    }

    /// Returns the validated viewer layout and density.
    #[must_use]
    pub const fn viewer_render_options(&self) -> RenderOptions {
        self.viewer_render_options
    }

    /// Returns the validated viewer search keyboard shortcuts.
    #[must_use]
    pub const fn viewer_keybindings(&self) -> ViewerKeybindings {
        self.viewer_keybindings
    }

    /// Returns whether a plain current-repository push requires confirmation.
    #[must_use]
    pub const fn push_confirmation_required(&self) -> bool {
        self.push_confirmation_required
    }

    /// Returns the complete validated project and default exclusion map.
    #[must_use]
    pub const fn diff_exclusions(&self) -> &DiffExclusions {
        &self.diff_exclusions
    }

    /// Returns the configured project names omitted from managed push fan-out.
    #[must_use]
    pub const fn push_all_exclusions(&self) -> &PushAllExclusions {
        &self.push_all_exclusions
    }

    #[must_use]
    pub const fn tag_patterns(&self) -> &TagPatternSettings {
        &self.tag_patterns
    }
}

#[cfg(test)]
mod tests {
    use super::{PushAllExclusions, UserSettings, UserSettingsRevision};
    use crate::{
        diffs::DiffExclusions,
        paths::{ProjectName, RepositoryRelativePath},
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme, ViewerKeybindings},
    };

    #[test]
    fn projects_page_size_rejects_invalid_construction_and_deserialization() {
        assert_eq!(super::ProjectsPageSize::default().into_inner(), 15);
        for value in [10, 15, 30] {
            let size = super::ProjectsPageSize::try_new(value).unwrap();
            let json = serde_json::to_value(size).unwrap();
            assert_eq!(json, serde_json::json!(value));
            assert_eq!(
                serde_json::from_value::<super::ProjectsPageSize>(json).unwrap(),
                size
            );
        }
        for value in [0, 1, 9, 11, 14, 16, 29, 31, u32::MAX] {
            assert!(super::ProjectsPageSize::try_new(value).is_err());
            assert!(
                serde_json::from_value::<super::ProjectsPageSize>(serde_json::json!(value))
                    .is_err()
            );
        }
        assert!(serde_json::from_str::<super::ProjectsPageSize>("\"15\"").is_err());
    }

    #[test]
    fn user_settings_revision_round_trips_canonical_sha256_hexadecimal() {
        let revision = UserSettingsRevision::from_digest([0xab; 32]);
        let encoded = revision.to_string();

        assert_eq!(encoded, "ab".repeat(32));
        assert_eq!(encoded.parse(), Ok(revision));
        assert_eq!(
            serde_json::to_string(&revision).unwrap(),
            format!("\"{encoded}\"")
        );
        assert_eq!(
            serde_json::from_str::<UserSettingsRevision>(&format!("\"{encoded}\"")).unwrap(),
            revision
        );
        for invalid in ["", "ab", &"AB".repeat(32), &"gg".repeat(32)] {
            assert!(invalid.parse::<UserSettingsRevision>().is_err());
        }
    }

    fn project(value: &str) -> ProjectName {
        ProjectName::try_new(value.to_owned()).unwrap()
    }

    fn path(value: &str) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(value.into()).unwrap()
    }

    #[test]
    fn accessors_expose_the_complete_immutable_snapshot() {
        let settings = UserSettings::new(
            Some(Theme::Hearth),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            ViewerKeybindings::default(),
            false,
            DiffExclusions::new([(project("git-tools"), vec!["md", "lock"])], None),
            PushAllExclusions::new([project("sample_project")]),
        );

        assert_eq!(settings.theme(), Some(Theme::Hearth));
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert_eq!(settings.viewer_keybindings(), ViewerKeybindings::default());
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
            ViewerKeybindings::default(),
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
