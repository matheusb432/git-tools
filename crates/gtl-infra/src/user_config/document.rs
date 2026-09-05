use std::{collections::BTreeMap, string::FromUtf8Error};

use gtl_application::settings::{
    ProjectSettingsUpdates, UserSettingsFieldUpdate, UserSettingsPatch,
};
use gtl_models::{
    diffs::{DiffExclusions, ExcludedExtensions},
    paths::{ProjectName, ProjectNameError},
    settings::{PushAllExclusions, UserSettings},
    viewer::{
        DiffDensity, DiffLayout, InvalidViewerKeybindings, ParseRenderOptionError,
        ParseViewerKeybindingError, RenderOptions, ViewerKeybinding, ViewerKeybindingAction,
        ViewerKeybindingPlatform, ViewerKeybindings,
    },
};
use serde::Deserialize;
use thiserror::Error;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub(super) enum UserSettingsDocumentKey {
    #[strum(to_string = "theme")]
    Theme,
    #[strum(to_string = "layout")]
    Layout,
    #[strum(to_string = "density")]
    Density,
    #[strum(to_string = "push.confirm")]
    PushConfirmation,
    #[strum(to_string = "keybindings.search_files")]
    KeybindingsSearchFiles,
    #[strum(to_string = "keybindings.search_text_in_all_files")]
    KeybindingsSearchTextInAllFiles,
    #[strum(to_string = "diff.exclude")]
    DefaultDiffExclusions,
    #[strum(to_string = "projects")]
    Projects,
    #[strum(to_string = "projects[{index}].name")]
    ProjectName { index: usize },
    #[strum(to_string = "projects[{index}].excluded_from_push_all")]
    ProjectExcludedFromPushAll { index: usize },
    #[strum(to_string = "projects[{index}].diff.exclude")]
    ProjectDiffExclusions { index: usize },
}

impl UserSettingsDocumentKey {
    const fn root(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::Density => "density",
            Self::PushConfirmation => "push",
            Self::KeybindingsSearchFiles | Self::KeybindingsSearchTextInAllFiles => "keybindings",
            Self::DefaultDiffExclusions => "diff",
            Self::Projects
            | Self::ProjectName { .. }
            | Self::ProjectExcludedFromPushAll { .. }
            | Self::ProjectDiffExclusions { .. } => "projects",
        }
    }

    const fn leaf(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::Density => "density",
            Self::PushConfirmation => "confirm",
            Self::KeybindingsSearchFiles => "search_files",
            Self::KeybindingsSearchTextInAllFiles => "search_text_in_all_files",
            Self::DefaultDiffExclusions | Self::ProjectDiffExclusions { .. } => "exclude",
            Self::Projects => "projects",
            Self::ProjectName { .. } => "name",
            Self::ProjectExcludedFromPushAll { .. } => "excluded_from_push_all",
        }
    }

    const fn container(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::Density => "density",
            Self::PushConfirmation => "push",
            Self::KeybindingsSearchFiles | Self::KeybindingsSearchTextInAllFiles => "keybindings",
            Self::DefaultDiffExclusions | Self::ProjectDiffExclusions { .. } => "diff",
            Self::Projects | Self::ProjectName { .. } | Self::ProjectExcludedFromPushAll { .. } => {
                "projects"
            }
        }
    }

    const fn nested(self) -> (&'static str, &'static str) {
        (self.container(), self.leaf())
    }
}

#[derive(Debug, Error)]
pub(super) enum UserSettingsDocumentError {
    #[error("user settings are not UTF-8: {0}")]
    Encoding(#[from] FromUtf8Error),
    #[error("user settings TOML syntax is invalid: {0}")]
    TomlSyntax(#[source] toml_edit::TomlError),
    #[error("user settings TOML schema is invalid: {0}")]
    TomlSchema(#[source] toml::de::Error),
    #[error("`{key}` must be a string")]
    ExpectedString { key: UserSettingsDocumentKey },
    #[error("`{key}` must be a boolean")]
    ExpectedBoolean { key: UserSettingsDocumentKey },
    #[error("`{key}` must be an array of strings")]
    ExpectedStringArray { key: UserSettingsDocumentKey },
    #[error("`{key}` must contain only strings")]
    ExpectedStringArrayElement { key: UserSettingsDocumentKey },
    #[error("`{key}` is required")]
    MissingField { key: UserSettingsDocumentKey },
    #[error("`{key}` is invalid: {source}")]
    InvalidRenderOption {
        key: UserSettingsDocumentKey,
        #[source]
        source: ParseRenderOptionError,
    },
    #[error("`{key}` is invalid: {source}")]
    InvalidKeybinding {
        key: UserSettingsDocumentKey,
        #[source]
        source: ParseViewerKeybindingError,
    },
    #[error("`{key}` is invalid on this platform: {source}")]
    InvalidKeybindingSet {
        key: UserSettingsDocumentKey,
        #[source]
        source: InvalidViewerKeybindings,
    },
    #[error("`{first}` conflicts with `{second}`: {source}")]
    ConflictingKeybindings {
        first: UserSettingsDocumentKey,
        second: UserSettingsDocumentKey,
        #[source]
        source: InvalidViewerKeybindings,
    },
    #[error("`{key}` is invalid: {source}")]
    InvalidProjectName {
        key: UserSettingsDocumentKey,
        #[source]
        source: ProjectNameError,
    },
    #[error("`{key}` contains duplicate project name `{name}`")]
    DuplicateProjectName {
        key: UserSettingsDocumentKey,
        name: ProjectName,
    },
}

impl UserSettingsDocumentError {
    pub(super) fn client_diagnostic(&self) -> Option<String> {
        matches!(
            self,
            Self::InvalidKeybinding { .. }
                | Self::InvalidKeybindingSet { .. }
                | Self::ConflictingKeybindings { .. }
        )
        .then(|| self.to_string())
    }
}

/// A TOML document whose complete supported schema and domain values are valid.
pub(super) struct UserSettingsDocument {
    raw: String,
    editable: DocumentMut,
    settings: UserSettings,
}

impl UserSettingsDocument {
    pub(super) fn parse(bytes: Vec<u8>) -> Result<Self, UserSettingsDocumentError> {
        let raw = String::from_utf8(bytes)?;
        let editable = raw
            .parse::<DocumentMut>()
            .map_err(UserSettingsDocumentError::TomlSyntax)?;
        let settings = parse_settings(&raw)?;
        Ok(Self {
            raw,
            editable,
            settings,
        })
    }

    pub(super) fn into_settings(self) -> UserSettings {
        self.settings
    }

    pub(super) fn apply(mut self, patch: UserSettingsPatch) -> UserSettingsDocumentEdit {
        apply_settings_patch(&mut self.editable, patch);
        let edited = self.editable.to_string();
        if edited == self.raw {
            UserSettingsDocumentEdit::Unchanged
        } else {
            UserSettingsDocumentEdit::Changed(edited)
        }
    }
}

pub(super) enum UserSettingsDocumentEdit {
    Changed(String),
    Unchanged,
}

type RawSettingValue = toml::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUserSettingsDocument {
    #[serde(default)]
    theme: Option<RawSettingValue>,
    #[serde(default)]
    layout: Option<RawSettingValue>,
    #[serde(default)]
    density: Option<RawSettingValue>,
    #[serde(default)]
    push: Option<RawPushSettingsDocument>,
    #[serde(default)]
    keybindings: Option<RawKeybindingsDocument>,
    #[serde(default)]
    diff: Option<RawDiffSettingsDocument>,
    #[serde(default)]
    projects: Vec<RawProjectSettingsDocument>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPushSettingsDocument {
    #[serde(default)]
    confirm: Option<RawSettingValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeybindingsDocument {
    #[serde(default)]
    search_files: Option<RawSettingValue>,
    #[serde(default)]
    search_text_in_all_files: Option<RawSettingValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiffSettingsDocument {
    #[serde(default)]
    exclude: Option<RawSettingValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProjectSettingsDocument {
    #[serde(default)]
    name: Option<RawSettingValue>,
    #[serde(default)]
    excluded_from_push_all: Option<RawSettingValue>,
    #[serde(default)]
    diff: Option<RawDiffSettingsDocument>,
}

fn parse_settings(raw: &str) -> Result<UserSettings, UserSettingsDocumentError> {
    let document = toml::from_str::<RawUserSettingsDocument>(raw)
        .map_err(UserSettingsDocumentError::TomlSchema)?;
    let theme = optional_string(UserSettingsDocumentKey::Theme, document.theme)?
        .map(|value| parse_render_option(UserSettingsDocumentKey::Theme, &value))
        .transpose()?;
    let layout = optional_string(UserSettingsDocumentKey::Layout, document.layout)?
        .map(|value| parse_render_option(UserSettingsDocumentKey::Layout, &value))
        .transpose()?
        .unwrap_or(DiffLayout::Unified);
    let density = optional_string(UserSettingsDocumentKey::Density, document.density)?
        .map(|value| parse_render_option(UserSettingsDocumentKey::Density, &value))
        .transpose()?
        .unwrap_or(DiffDensity::Compact);
    let keybindings = parse_keybindings(document.keybindings.unwrap_or_default())?;
    let push_confirmation_required = optional_bool(
        UserSettingsDocumentKey::PushConfirmation,
        document.push.and_then(|push| push.confirm),
    )?
    .unwrap_or(UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT);
    let diff_exclusions_default = excluded_extensions(
        UserSettingsDocumentKey::DefaultDiffExclusions,
        document.diff.and_then(|diff| diff.exclude),
    )?;
    let (diff_exclusions_projects, push_all_exclusions) = project_settings(document.projects)?;

    Ok(UserSettings::new(
        theme,
        RenderOptions::new(layout, density),
        keybindings,
        push_confirmation_required,
        DiffExclusions::new(diff_exclusions_projects, diff_exclusions_default),
        push_all_exclusions,
    ))
}

fn parse_keybindings(
    document: RawKeybindingsDocument,
) -> Result<ViewerKeybindings, UserSettingsDocumentError> {
    let platform = ViewerKeybindingPlatform::current();
    let defaults = ViewerKeybindings::for_platform(platform);
    let search_files = optional_keybinding(
        UserSettingsDocumentKey::KeybindingsSearchFiles,
        document.search_files,
    )?
    .unwrap_or(defaults[ViewerKeybindingAction::SearchFiles]);
    let search_text_in_all_files = optional_keybinding(
        UserSettingsDocumentKey::KeybindingsSearchTextInAllFiles,
        document.search_text_in_all_files,
    )?
    .unwrap_or(defaults[ViewerKeybindingAction::SearchTextInAllFiles]);

    ViewerKeybindings::try_from_fn(platform, |action| match action {
        ViewerKeybindingAction::SearchFiles => search_files,
        ViewerKeybindingAction::SearchTextInAllFiles => search_text_in_all_files,
    })
    .map_err(|source| match source {
        InvalidViewerKeybindings::AmbiguousMacOsModifiers { action } => {
            UserSettingsDocumentError::InvalidKeybindingSet {
                key: keybinding_document_key(action),
                source,
            }
        }
        InvalidViewerKeybindings::Conflict { first, second, .. } => {
            UserSettingsDocumentError::ConflictingKeybindings {
                first: keybinding_document_key(first),
                second: keybinding_document_key(second),
                source,
            }
        }
    })
}

const fn keybinding_document_key(action: ViewerKeybindingAction) -> UserSettingsDocumentKey {
    match action {
        ViewerKeybindingAction::SearchFiles => UserSettingsDocumentKey::KeybindingsSearchFiles,
        ViewerKeybindingAction::SearchTextInAllFiles => {
            UserSettingsDocumentKey::KeybindingsSearchTextInAllFiles
        }
    }
}

fn optional_keybinding(
    key: UserSettingsDocumentKey,
    value: Option<RawSettingValue>,
) -> Result<Option<ViewerKeybinding>, UserSettingsDocumentError> {
    optional_string(key, value)?
        .map(|value| {
            value
                .parse()
                .map_err(|source| UserSettingsDocumentError::InvalidKeybinding { key, source })
        })
        .transpose()
}

fn parse_render_option<T>(
    key: UserSettingsDocumentKey,
    value: &str,
) -> Result<T, UserSettingsDocumentError>
where
    T: std::str::FromStr<Err = ParseRenderOptionError>,
{
    value
        .parse()
        .map_err(|source| UserSettingsDocumentError::InvalidRenderOption { key, source })
}

fn optional_string(
    key: UserSettingsDocumentKey,
    value: Option<RawSettingValue>,
) -> Result<Option<String>, UserSettingsDocumentError> {
    value
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(UserSettingsDocumentError::ExpectedString { key })
        })
        .transpose()
}

fn optional_bool(
    key: UserSettingsDocumentKey,
    value: Option<RawSettingValue>,
) -> Result<Option<bool>, UserSettingsDocumentError> {
    value
        .map(|value| {
            value
                .as_bool()
                .ok_or(UserSettingsDocumentError::ExpectedBoolean { key })
        })
        .transpose()
}

fn excluded_extensions(
    key: UserSettingsDocumentKey,
    value: Option<RawSettingValue>,
) -> Result<Option<Vec<String>>, UserSettingsDocumentError> {
    value.map(|value| string_array(key, value)).transpose()
}

fn string_array(
    key: UserSettingsDocumentKey,
    value: RawSettingValue,
) -> Result<Vec<String>, UserSettingsDocumentError> {
    let RawSettingValue::Array(values) = value else {
        return Err(UserSettingsDocumentError::ExpectedStringArray { key });
    };
    values
        .into_iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(UserSettingsDocumentError::ExpectedStringArrayElement { key })
        })
        .collect()
}

fn project_settings(
    projects: Vec<RawProjectSettingsDocument>,
) -> Result<(BTreeMap<ProjectName, Vec<String>>, PushAllExclusions), UserSettingsDocumentError> {
    let mut diff_exclusions = BTreeMap::new();
    let mut push_all_exclusions = Vec::new();
    let mut project_names = std::collections::BTreeSet::new();

    for (index, project) in projects.into_iter().enumerate() {
        let name_key = UserSettingsDocumentKey::ProjectName { index };
        let name = optional_string(name_key, project.name)?
            .ok_or(UserSettingsDocumentError::MissingField { key: name_key })?;
        let name = ProjectName::try_new(name).map_err(|source| {
            UserSettingsDocumentError::InvalidProjectName {
                key: name_key,
                source,
            }
        })?;
        if !project_names.insert(name.clone()) {
            return Err(UserSettingsDocumentError::DuplicateProjectName {
                key: UserSettingsDocumentKey::Projects,
                name,
            });
        }

        let diff_key = UserSettingsDocumentKey::ProjectDiffExclusions { index };
        if let Some(exclusions) =
            excluded_extensions(diff_key, project.diff.and_then(|diff| diff.exclude))?
        {
            diff_exclusions.insert(name.clone(), exclusions);
        }
        if optional_bool(
            UserSettingsDocumentKey::ProjectExcludedFromPushAll { index },
            project.excluded_from_push_all,
        )?
        .unwrap_or(false)
        {
            push_all_exclusions.push(name);
        }
    }

    Ok((diff_exclusions, PushAllExclusions::new(push_all_exclusions)))
}

fn apply_settings_patch(document: &mut DocumentMut, patch: UserSettingsPatch) {
    apply_root_string(document, UserSettingsDocumentKey::Theme, patch.theme);
    apply_root_string(document, UserSettingsDocumentKey::Layout, patch.layout);
    apply_root_string(document, UserSettingsDocumentKey::Density, patch.density);
    apply_nested_bool(
        document,
        UserSettingsDocumentKey::PushConfirmation,
        &patch.push_confirmation_required,
    );
    apply_nested_exclusions(
        document,
        UserSettingsDocumentKey::DefaultDiffExclusions,
        patch.default_diff_exclusions,
    );
    apply_projects(document, patch.projects);
}

fn apply_root_string<T: ToString>(
    document: &mut DocumentMut,
    key: UserSettingsDocumentKey,
    update: UserSettingsFieldUpdate<T>,
) {
    match update {
        UserSettingsFieldUpdate::Update(value) => {
            set_value(&mut document[key.root()], Value::from(value.to_string()));
        }
        UserSettingsFieldUpdate::Clear => {
            document.remove(key.root());
        }
        UserSettingsFieldUpdate::Unchanged => {}
    }
}

fn apply_nested_bool(
    document: &mut DocumentMut,
    key: UserSettingsDocumentKey,
    update: &UserSettingsFieldUpdate<bool>,
) {
    match update {
        UserSettingsFieldUpdate::Update(value) => {
            let (section, field) = key.nested();
            set_value(&mut document[section][field], Value::from(*value));
        }
        UserSettingsFieldUpdate::Clear => remove_nested(document, key),
        UserSettingsFieldUpdate::Unchanged => {}
    }
}

fn apply_nested_exclusions(
    document: &mut DocumentMut,
    key: UserSettingsDocumentKey,
    update: UserSettingsFieldUpdate<ExcludedExtensions>,
) {
    match update {
        UserSettingsFieldUpdate::Update(extensions) => {
            let (section, field) = key.nested();
            set_value(
                &mut document[section][field],
                Value::Array(extension_array(&extensions)),
            );
        }
        UserSettingsFieldUpdate::Clear => remove_nested(document, key),
        UserSettingsFieldUpdate::Unchanged => {}
    }
}

fn apply_projects(
    document: &mut DocumentMut,
    update: UserSettingsFieldUpdate<ProjectSettingsUpdates>,
) {
    match update {
        UserSettingsFieldUpdate::Update(projects) => {
            let mut tables = ArrayOfTables::new();
            for (index, project) in projects.into_iter().enumerate() {
                let mut table = Table::new();
                table[UserSettingsDocumentKey::ProjectName { index }.leaf()] =
                    toml_edit::value(project.name.to_string());
                table[UserSettingsDocumentKey::ProjectExcludedFromPushAll { index }.leaf()] =
                    toml_edit::value(project.excluded_from_push_all);
                let mut diff = Table::new();
                diff[UserSettingsDocumentKey::ProjectDiffExclusions { index }.leaf()] =
                    Item::Value(Value::Array(extension_array(&project.diff_exclusions)));
                table[UserSettingsDocumentKey::ProjectDiffExclusions { index }.container()] =
                    Item::Table(diff);
                tables.push(table);
            }
            document[UserSettingsDocumentKey::Projects.root()] = Item::ArrayOfTables(tables);
        }
        UserSettingsFieldUpdate::Clear => {
            document.remove(UserSettingsDocumentKey::Projects.root());
        }
        UserSettingsFieldUpdate::Unchanged => {}
    }
}

fn extension_array(extensions: &ExcludedExtensions) -> Array {
    let mut values = Array::new();
    for extension in extensions.extensions() {
        values.push(extension.as_str());
    }
    values
}

fn set_value(item: &mut Item, mut replacement: Value) {
    if let Item::Value(current) = item {
        if same_value(current, &replacement) {
            return;
        }
        *replacement.decor_mut() = current.decor().clone();
    }
    *item = Item::Value(replacement);
}

fn same_value(current: &Value, replacement: &Value) -> bool {
    match (current, replacement) {
        (Value::String(current), Value::String(replacement)) => {
            current.value() == replacement.value()
        }
        (Value::Boolean(current), Value::Boolean(replacement)) => {
            current.value() == replacement.value()
        }
        (Value::Array(current), Value::Array(replacement)) => current
            .iter()
            .map(Value::as_str)
            .eq(replacement.iter().map(Value::as_str)),
        _ => false,
    }
}

fn remove_nested(document: &mut DocumentMut, key: UserSettingsDocumentKey) {
    let (section, field) = key.nested();
    if let Some(table) = document.get_mut(section).and_then(Item::as_table_mut) {
        table.remove(field);
    } else if let Some(table) = document
        .get_mut(section)
        .and_then(Item::as_inline_table_mut)
    {
        table.remove(field);
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::settings::{
        ProjectSettingsUpdate, ProjectSettingsUpdates, UserSettingsFieldUpdate, UserSettingsPatch,
    };
    use gtl_models::{
        diffs::ExcludedExtensions,
        paths::ProjectName,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    use super::{
        UserSettingsDocument, UserSettingsDocumentEdit, UserSettingsDocumentError,
        UserSettingsDocumentKey,
    };

    #[test]
    fn key_paths_are_explicit_and_complete() {
        assert_eq!(UserSettingsDocumentKey::Theme.to_string(), "theme");
        assert_eq!(UserSettingsDocumentKey::Layout.to_string(), "layout");
        assert_eq!(UserSettingsDocumentKey::Density.to_string(), "density");
        assert_eq!(
            UserSettingsDocumentKey::PushConfirmation.to_string(),
            "push.confirm"
        );
        assert_eq!(
            UserSettingsDocumentKey::DefaultDiffExclusions.to_string(),
            "diff.exclude"
        );
        assert_eq!(UserSettingsDocumentKey::Projects.to_string(), "projects");
        assert_eq!(
            UserSettingsDocumentKey::ProjectName { index: 3 }.to_string(),
            "projects[3].name"
        );
        assert_eq!(
            UserSettingsDocumentKey::ProjectExcludedFromPushAll { index: 3 }.to_string(),
            "projects[3].excluded_from_push_all"
        );
        assert_eq!(
            UserSettingsDocumentKey::ProjectDiffExclusions { index: 3 }.to_string(),
            "projects[3].diff.exclude"
        );
    }

    #[test]
    fn parse_closes_the_document_over_schema_and_domain_values() {
        let document = UserSettingsDocument::parse(
            br#"
theme = "light"
layout = "split"
density = "full"
push = { confirm = false }
diff = { exclude = ["md"] }

[[projects]]
name = "git-tools"
excluded_from_push_all = true
diff = { exclude = ["js"] }
"#
            .to_vec(),
        )
        .unwrap();
        let settings = document.into_settings();

        assert_eq!(settings.theme(), Some(Theme::Light));
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert!(!settings.push_confirmation_required());
        assert!(
            settings
                .push_all_exclusions()
                .contains(&ProjectName::try_from("git-tools").unwrap())
        );
    }

    #[test]
    fn parse_reports_field_specific_failures_without_stringly_typed_reasons() {
        let error = UserSettingsDocument::parse(b"[push]\nconfirm = \"yes\"\n".to_vec())
            .err()
            .unwrap();

        assert!(matches!(
            error,
            UserSettingsDocumentError::ExpectedBoolean {
                key: UserSettingsDocumentKey::PushConfirmation
            }
        ));
    }

    #[test]
    fn parse_preserves_each_document_failure_category() {
        assert!(matches!(
            UserSettingsDocument::parse(vec![0xff]).err().unwrap(),
            UserSettingsDocumentError::Encoding(_)
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"theme = {{{\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::TomlSyntax(_)
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"unknown = true\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::TomlSchema(_)
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"theme = 3\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::ExpectedString {
                key: UserSettingsDocumentKey::Theme
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"[diff]\nexclude = true\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::ExpectedStringArray {
                key: UserSettingsDocumentKey::DefaultDiffExclusions
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"[diff]\nexclude = [\"md\", 3]\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::ExpectedStringArrayElement {
                key: UserSettingsDocumentKey::DefaultDiffExclusions
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"[[projects]]\nexcluded_from_push_all = true\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::MissingField {
                key: UserSettingsDocumentKey::ProjectName { index: 0 }
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"theme = \"sideways\"\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::InvalidRenderOption {
                key: UserSettingsDocumentKey::Theme,
                ..
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(b"[[projects]]\nname = \"\"\n".to_vec())
                .err()
                .unwrap(),
            UserSettingsDocumentError::InvalidProjectName {
                key: UserSettingsDocumentKey::ProjectName { index: 0 },
                ..
            }
        ));
        assert!(matches!(
            UserSettingsDocument::parse(
                b"[[projects]]\nname = \"same\"\n[[projects]]\nname = \"same\"\n".to_vec()
            )
            .err()
            .unwrap(),
            UserSettingsDocumentError::DuplicateProjectName {
                key: UserSettingsDocumentKey::Projects,
                ..
            }
        ));
    }

    #[test]
    fn equal_explicit_scalar_is_an_exact_no_op() {
        let document = UserSettingsDocument::parse(b"theme = 'dark'\n".to_vec()).unwrap();
        let patch = UserSettingsPatch {
            theme: UserSettingsFieldUpdate::Update(Theme::Dark),
            ..UserSettingsPatch::default()
        };

        assert!(matches!(
            document.apply(patch),
            UserSettingsDocumentEdit::Unchanged
        ));
    }

    #[test]
    fn typed_patch_handles_inline_tables_and_preserves_an_equal_scalar() {
        let document = UserSettingsDocument::parse(
            b"theme = 'dark'\npush = { confirm = true }\ndiff = { exclude = [\"md\"] }\n".to_vec(),
        )
        .unwrap();
        let patch = UserSettingsPatch {
            theme: UserSettingsFieldUpdate::Update(Theme::Dark),
            push_confirmation_required: UserSettingsFieldUpdate::Clear,
            default_diff_exclusions: UserSettingsFieldUpdate::Update(ExcludedExtensions::new([
                "md",
            ])),
            ..UserSettingsPatch::default()
        };

        let edit = document.apply(patch);
        assert!(matches!(&edit, UserSettingsDocumentEdit::Changed(_)));
        let UserSettingsDocumentEdit::Changed(raw) = edit else {
            return;
        };
        assert!(raw.contains("theme = 'dark'"));
        assert!(raw.contains("push = {}"));
        assert!(raw.contains("diff = { exclude = [\"md\"] }"));
    }

    #[test]
    fn project_replacement_uses_the_checked_ordered_collection() {
        let document = UserSettingsDocument::parse(Vec::new()).unwrap();
        let projects = ProjectSettingsUpdates::try_new([ProjectSettingsUpdate {
            name: ProjectName::try_from("git-tools").unwrap(),
            excluded_from_push_all: true,
            diff_exclusions: ExcludedExtensions::new(["lock"]),
        }])
        .unwrap();
        let patch = UserSettingsPatch {
            projects: UserSettingsFieldUpdate::Update(projects),
            ..UserSettingsPatch::default()
        };

        let edit = document.apply(patch);
        assert!(matches!(&edit, UserSettingsDocumentEdit::Changed(_)));
        let UserSettingsDocumentEdit::Changed(raw) = edit else {
            return;
        };
        assert!(raw.contains("[[projects]]"));
        assert!(raw.contains("name = \"git-tools\""));
    }
}
