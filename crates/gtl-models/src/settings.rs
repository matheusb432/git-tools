//! Immutable, validated user settings used by application operations.

pub mod setting_key;

use std::collections::BTreeSet;

pub use setting_key::*;

use crate::{
    paths::ProjectName,
    tags::TagPatternSettings,
    viewer::{RenderOptions, Theme, ViewerKeybindings},
};

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

/// Orders the changed files of a diff in the viewer.
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
pub enum DiffFilesSort {
    /// Directories before files at each level, then by name.
    #[default]
    Path,
    /// Most added plus removed lines first.
    Changes,
}

#[nutype::nutype(
    validate(predicate = |value| matches!(value, 10 | 15 | 30)),
    default = 15,
    derive(Debug, Clone, Copy, Default, PartialEq, Eq, Display, Serialize, Deserialize)
)]
pub struct ProjectsPageSize(u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectsPreferences {
    pub page_size: ProjectsPageSize,
    pub sort: ProjectsSort,
}

#[nutype::nutype(
    validate(predicate = |value| (100..=300).contains(value) && value.is_multiple_of(25)),
    default = 100,
    derive(Debug, Clone, Copy, Default, PartialEq, Eq, Display, Serialize, Deserialize)
)]
pub struct ViewerScalePercent(u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerAccessibility {
    pub ui_scale_percent: ViewerScalePercent,
    /// Also honors the system preference when false.
    pub reduce_motion: bool,
}

/// Selects the language of the viewer and of the offline artifacts it renders.
///
/// The persisted token is the variant's BCP 47 language tag.
///
/// # Examples
///
/// ```
/// use gtl_models::settings::ViewerLanguage;
///
/// assert_eq!("pt-BR".parse(), Ok(ViewerLanguage::PtBr));
/// assert_eq!(ViewerLanguage::default().as_str(), "en-US");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, strum::VariantArray)]
pub enum ViewerLanguage {
    #[default]
    EnUs,
    PtBr,
}

impl ViewerLanguage {
    /// Every supported language, in declaration order.
    pub const ALL: &'static [Self] = <Self as strum::VariantArray>::VARIANTS;

    /// Returns the BCP 47 language tag that persists and identifies this language.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::PtBr => "pt-BR",
        }
    }
}

impl std::fmt::Display for ViewerLanguage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ViewerLanguage {
    type Err = ParseViewerLanguageError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|language| language.as_str() == raw)
            .ok_or_else(|| ParseViewerLanguageError {
                value: raw.to_owned(),
            })
    }
}

impl serde::Serialize for ViewerLanguage {
    fn serialize<SerializerType>(
        &self,
        serializer: SerializerType,
    ) -> Result<SerializerType::Ok, SerializerType::Error>
    where
        SerializerType: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ViewerLanguage {
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

/// Reports a language tag that names no supported viewer language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown viewer language `{value}`; expected `en-US` or `pt-BR`")]
pub struct ParseViewerLanguageError {
    value: String,
}

/// Selects how the viewer displays dates.
///
/// The viewer shows every format in the local time zone. Offline artifacts ignore
/// this setting and keep [`Self::Iso`] in each timestamp's recorded offset.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    strum::Display,
    strum::EnumString,
    strum::VariantArray,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ViewerDateFormat {
    /// `2026-06-28 13:45`
    #[default]
    Iso,
    /// `28/06/2026 13:45`
    DayFirst,
    /// `06/28/2026 1:45 PM`
    MonthFirst,
    /// `3 hours ago` within the last seven days, and [`Self::Iso`] otherwise.
    Relative,
}

impl ViewerDateFormat {
    /// Every supported format, in declaration order.
    pub const ALL: &'static [Self] = <Self as strum::VariantArray>::VARIANTS;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PushConfirmationPreferences {
    pub cli_required: bool,
    pub viewer_required: bool,
}

impl Default for PushConfirmationPreferences {
    fn default() -> Self {
        Self {
            cli_required: UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT,
            viewer_required: UserSettings::VIEWER_PUSH_CONFIRMATION_REQUIRED_DEFAULT,
        }
    }
}

// TODO: remove Clone once a store-owned smart pointer is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSettings {
    accessibility: ViewerAccessibility,
    language: ViewerLanguage,
    date_format: ViewerDateFormat,
    focus_window_on_diff: bool,
    copy_with_line_context: bool,
    diff_files_sort: DiffFilesSort,
    theme: Option<Theme>,
    viewer_render_options: RenderOptions,
    viewer_keybindings: ViewerKeybindings,
    sidebar_visibility: crate::viewer::ViewerSidebarVisibility,
    push_confirmation: PushConfirmationPreferences,
    viewer_push_no_confirmation_projects: BTreeSet<ProjectName>,
    push_all_exclusions: PushAllExclusions,
    tag_patterns: TagPatternSettings,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self::new(
            None,
            RenderOptions::DEFAULT,
            ViewerKeybindings::default(),
            Self::PUSH_CONFIRMATION_REQUIRED_DEFAULT,
            PushAllExclusions::default(),
        )
    }
}

impl UserSettings {
    pub const PUSH_CONFIRMATION_REQUIRED_DEFAULT: bool = true;
    pub const VIEWER_PUSH_CONFIRMATION_REQUIRED_DEFAULT: bool = true;
    pub const COPY_WITH_LINE_CONTEXT_DEFAULT: bool = true;

    /// Constructs a complete settings snapshot from validated values.
    #[must_use]
    pub fn new(
        theme: Option<Theme>,
        viewer_render_options: RenderOptions,
        viewer_keybindings: ViewerKeybindings,
        push_confirmation_required: bool,
        push_all_exclusions: PushAllExclusions,
    ) -> Self {
        Self {
            accessibility: ViewerAccessibility::default(),
            language: ViewerLanguage::default(),
            date_format: ViewerDateFormat::default(),
            focus_window_on_diff: true,
            copy_with_line_context: Self::COPY_WITH_LINE_CONTEXT_DEFAULT,
            diff_files_sort: DiffFilesSort::default(),
            theme,
            viewer_render_options,
            viewer_keybindings,
            sidebar_visibility: crate::viewer::ViewerSidebarVisibility::default(),
            push_confirmation: PushConfirmationPreferences {
                cli_required: push_confirmation_required,
                ..PushConfirmationPreferences::default()
            },
            viewer_push_no_confirmation_projects: BTreeSet::new(),
            push_all_exclusions,
            tag_patterns: TagPatternSettings::default(),
        }
    }

    #[must_use]
    pub fn with_accessibility(self, accessibility: ViewerAccessibility) -> Self {
        Self {
            accessibility,
            ..self
        }
    }

    #[must_use]
    pub const fn accessibility(&self) -> ViewerAccessibility {
        self.accessibility
    }

    #[must_use]
    pub fn with_language(self, language: ViewerLanguage) -> Self {
        Self { language, ..self }
    }

    #[must_use]
    pub const fn language(&self) -> ViewerLanguage {
        self.language
    }

    #[must_use]
    pub fn with_date_format(self, date_format: ViewerDateFormat) -> Self {
        Self {
            date_format,
            ..self
        }
    }

    #[must_use]
    pub const fn date_format(&self) -> ViewerDateFormat {
        self.date_format
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
    pub fn with_copy_with_line_context(self, copy_with_line_context: bool) -> Self {
        Self {
            copy_with_line_context,
            ..self
        }
    }

    #[must_use]
    pub const fn copy_with_line_context(&self) -> bool {
        self.copy_with_line_context
    }

    #[must_use]
    pub fn with_diff_files_sort(self, diff_files_sort: DiffFilesSort) -> Self {
        Self {
            diff_files_sort,
            ..self
        }
    }

    #[must_use]
    pub const fn diff_files_sort(&self) -> DiffFilesSort {
        self.diff_files_sort
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

    #[must_use]
    pub fn with_theme(self, theme: Option<Theme>) -> Self {
        Self { theme, ..self }
    }

    #[must_use]
    pub fn with_viewer_render_options(self, viewer_render_options: RenderOptions) -> Self {
        Self {
            viewer_render_options,
            ..self
        }
    }

    #[must_use]
    pub fn with_push_confirmation_required(mut self, push_confirmation_required: bool) -> Self {
        self.push_confirmation.cli_required = push_confirmation_required;
        self
    }

    #[must_use]
    pub fn with_viewer_push_confirmation_required(
        mut self,
        viewer_push_confirmation_required: bool,
    ) -> Self {
        self.push_confirmation.viewer_required = viewer_push_confirmation_required;
        self
    }

    #[must_use]
    pub fn with_viewer_push_no_confirmation_projects(
        self,
        projects: BTreeSet<ProjectName>,
    ) -> Self {
        Self {
            viewer_push_no_confirmation_projects: projects,
            ..self
        }
    }

    #[must_use]
    pub const fn viewer_push_no_confirmation_projects(&self) -> &BTreeSet<ProjectName> {
        &self.viewer_push_no_confirmation_projects
    }

    #[must_use]
    pub fn with_push_all_exclusions(self, push_all_exclusions: PushAllExclusions) -> Self {
        Self {
            push_all_exclusions,
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
        self.push_confirmation.cli_required
    }

    /// Returns whether viewer pushes require confirmation unless the project opts out.
    #[must_use]
    pub const fn viewer_push_confirmation_required(&self) -> bool {
        self.push_confirmation.viewer_required
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
        paths::ProjectName,
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
    fn viewer_language_serializes_as_its_exact_language_tag() {
        for language in super::ViewerLanguage::ALL {
            let json = serde_json::to_value(language).unwrap();
            assert_eq!(json, serde_json::json!(language.as_str()));
            assert_eq!(
                serde_json::from_value::<super::ViewerLanguage>(json).unwrap(),
                *language
            );
        }
        for invalid in ["pt-br", "en", "fr", ""] {
            assert!(invalid.parse::<super::ViewerLanguage>().is_err());
            assert!(
                serde_json::from_value::<super::ViewerLanguage>(serde_json::json!(invalid))
                    .is_err()
            );
        }
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

    #[test]
    fn accessors_expose_the_complete_immutable_snapshot() {
        let settings = UserSettings::new(
            Some(Theme::Mirage),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            ViewerKeybindings::default(),
            false,
            PushAllExclusions::new([project("sample_project")]),
        );

        assert_eq!(settings.theme(), Some(Theme::Mirage));
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
    }

    #[test]
    fn constructor_preserves_an_absent_theme_and_validated_defaults() {
        let settings = UserSettings::new(
            None,
            RenderOptions::DEFAULT,
            ViewerKeybindings::default(),
            true,
            PushAllExclusions::default(),
        );

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(settings.push_all_exclusions().is_empty());
    }
}
