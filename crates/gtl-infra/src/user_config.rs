//! TOML-backed user settings used by the server process root.

mod string_editor;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use gtl_application::{
    ports::{
        UserSettingsEditError, UserSettingsEditOutcome, UserSettingsLoadError, UserSettingsStore,
    },
    settings::edit_settings::{EditSettingsRequest, FieldUpdate},
};
use gtl_models::{
    diffs::DiffExclusions,
    paths::ProjectName,
    settings::{PushAllExclusions, UserSettings},
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};
use gtl_wire::settings::{ProjectSettingsDocument, RawSettingValue, UserSettingsDocument};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

fn default_settings() -> UserSettings {
    UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT,
        DiffExclusions::default(),
        PushAllExclusions::default(),
    )
}

fn invalid_configuration(path: &Path, reason: impl Into<String>) -> UserSettingsLoadError {
    UserSettingsLoadError::InvalidConfiguration {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn optional_string(
    path: &Path,
    field: &str,
    value: Option<RawSettingValue>,
) -> Result<Option<String>, UserSettingsLoadError> {
    value
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_configuration(path, format!("`{field}` must be a string")))
        })
        .transpose()
}

fn optional_bool(
    path: &Path,
    field: &str,
    value: Option<RawSettingValue>,
) -> Result<Option<bool>, UserSettingsLoadError> {
    value
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid_configuration(path, format!("`{field}` must be a boolean")))
        })
        .transpose()
}

fn excluded_extensions(
    path: &Path,
    field: &str,
    value: Option<RawSettingValue>,
) -> Result<Option<Vec<String>>, UserSettingsLoadError> {
    value
        .map(|value| string_array(path, field, value))
        .transpose()
}

fn string_array(
    path: &Path,
    field: &str,
    value: RawSettingValue,
) -> Result<Vec<String>, UserSettingsLoadError> {
    let RawSettingValue::Array(values) = value else {
        return Err(invalid_configuration(
            path,
            format!("`{field}` must be an array of strings"),
        ));
    };
    values
        .into_iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                invalid_configuration(path, format!("`{field}` must contain only strings"))
            })
        })
        .collect()
}

fn project_settings(
    path: &Path,
    projects: Vec<ProjectSettingsDocument>,
) -> Result<(BTreeMap<ProjectName, Vec<String>>, PushAllExclusions), UserSettingsLoadError> {
    let mut diff_exclusions = BTreeMap::new();
    let mut push_all_exclusions = Vec::new();
    let mut project_names = std::collections::BTreeSet::new();

    for (index, project) in projects.into_iter().enumerate() {
        let name_field = format!("projects[{index}].name");
        let name = optional_string(path, &name_field, project.name)?
            .ok_or_else(|| invalid_configuration(path, format!("`{name_field}` is required")))?;
        let name = ProjectName::try_new(name).map_err(|error| {
            invalid_configuration(path, format!("`{name_field}` is invalid: {error}"))
        })?;
        if !project_names.insert(name.clone()) {
            return Err(invalid_configuration(
                path,
                format!("`projects` contains duplicate project name `{name}`"),
            ));
        }

        let diff_field = format!("projects[{index}].diff.exclude");
        if let Some(exclusions) = excluded_extensions(
            path,
            &diff_field,
            project.diff.and_then(|diff| diff.exclude),
        )? {
            diff_exclusions.insert(name.clone(), exclusions);
        }
        let push_field = format!("projects[{index}].excluded_from_push_all");
        if optional_bool(path, &push_field, project.excluded_from_push_all)?.unwrap_or(false) {
            push_all_exclusions.push(name);
        }
    }

    Ok((diff_exclusions, PushAllExclusions::new(push_all_exclusions)))
}

fn parse_settings(path: &Path, raw: &str) -> Result<UserSettings, UserSettingsLoadError> {
    let document = toml::from_str::<UserSettingsDocument>(raw)
        .map_err(|error| invalid_configuration(path, error.to_string()))?;
    let theme = optional_string(path, "theme", document.theme)?
        .map(|value| {
            value
                .parse::<Theme>()
                .map_err(|error| invalid_configuration(path, error.to_string()))
        })
        .transpose()?;
    let layout = optional_string(path, "layout", document.layout)?
        .map(|value| {
            value
                .parse::<DiffLayout>()
                .map_err(|error| invalid_configuration(path, error.to_string()))
        })
        .transpose()?
        .unwrap_or(DiffLayout::Unified);
    let density = optional_string(path, "density", document.density)?
        .map(|value| {
            value
                .parse::<DiffDensity>()
                .map_err(|error| invalid_configuration(path, error.to_string()))
        })
        .transpose()?
        .unwrap_or(DiffDensity::Compact);
    let push_confirmation_required = optional_bool(
        path,
        "push.confirm",
        document.push.and_then(|push| push.confirm),
    )?
    .unwrap_or(UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT);
    let diff_exclusions_default = excluded_extensions(
        path,
        "diff.exclude",
        document.diff.and_then(|diff| diff.exclude),
    )?;
    let (diff_exclusions_projects, push_all_exclusions) =
        project_settings(path, document.projects)?;
    let diff_exclusions = DiffExclusions::new(diff_exclusions_projects, diff_exclusions_default);

    Ok(UserSettings::new(
        theme,
        RenderOptions::new(layout, density),
        push_confirmation_required,
        diff_exclusions,
        push_all_exclusions,
    ))
}

pub(super) fn validate_raw_for_edit(path: &Path, raw: &str) -> Result<(), UserSettingsEditError> {
    parse_settings(path, raw)
        .map(|_| ())
        .map_err(|error| match error {
            UserSettingsLoadError::InvalidConfiguration { path, reason } => {
                UserSettingsEditError::InvalidConfiguration { path, reason }
            }
            UserSettingsLoadError::Read { source, .. } => UserSettingsEditError::Unexpected(
                anyhow::Error::new(source).context("validate user settings before editing"),
            ),
            error => UserSettingsEditError::Unexpected(
                anyhow::Error::new(error).context("validate user settings before editing"),
            ),
        })
}

fn load_from(path: Option<&Path>) -> Result<UserSettings, UserSettingsLoadError> {
    let Some(path) = path else {
        return Ok(default_settings());
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            let raw = String::from_utf8(bytes)
                .map_err(|error| invalid_configuration(path, error.to_string()))?;
            parse_settings(path, &raw)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_settings()),
        Err(source) => Err(UserSettingsLoadError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// TOML-backed user settings for one resolved configuration path.
#[derive(Debug, Clone)]
pub struct TomlSettingsStore {
    path: Option<PathBuf>,
}

impl TomlSettingsStore {
    /// Creates a store for an explicit path, or an unresolved production path.
    #[must_use]
    pub const fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// Resolves the production configuration path from the process environment.
    #[must_use]
    pub fn from_environment() -> Self {
        Self::new(config_path())
    }

    /// Returns the resolved configuration path when one is available.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn required_path(&self) -> anyhow::Result<&Path> {
        self.path.as_deref().context(
            "could not resolve a config path (no GIT_TOOLS_CONFIG, XDG_CONFIG_HOME, or HOME)",
        )
    }
}

impl UserSettingsStore for TomlSettingsStore {
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
        load_from(self.path.as_deref())
    }

    fn set_value(
        &mut self,
        mutation: gtl_models::settings::SettingKeyValue,
    ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
        let key = mutation.key();
        let value_new = mutation.value();
        string_editor::edit(
            self.required_path()?,
            key.as_str(),
            string_editor::StringEdit::Set(&value_new),
        )
        .map(Into::into)
    }

    fn remove_key(
        &mut self,
        key: gtl_models::settings::SettingKey,
    ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
        string_editor::edit(
            self.required_path()?,
            key.as_str(),
            string_editor::StringEdit::Remove,
        )
        .map(Into::into)
    }

    fn edit_settings(
        &mut self,
        request: EditSettingsRequest,
    ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
        string_editor::edit_document(self.required_path()?, move |document| {
            apply_settings_patch(document, request);
            Ok(())
        })
    }
}

fn apply_scalar<T: ToString>(document: &mut DocumentMut, key: &str, update: FieldUpdate<T>) {
    match update {
        FieldUpdate::Update(value) => document[key] = toml_edit::value(value.to_string()),
        FieldUpdate::Clear => {
            document.remove(key);
        }
        FieldUpdate::Unchanged => {}
    }
}

fn apply_settings_patch(document: &mut DocumentMut, request: EditSettingsRequest) {
    apply_scalar(document, "theme", request.theme);
    apply_scalar(document, "layout", request.layout);
    apply_scalar(document, "density", request.density);
    match request.push_confirmation_required {
        FieldUpdate::Update(value) => {
            if !document.contains_key("push") {
                document["push"] = Item::Table(Table::new());
            }
            document["push"]["confirm"] = toml_edit::value(value);
        }
        FieldUpdate::Clear => {
            if let Some(push) = document.get_mut("push").and_then(Item::as_table_mut) {
                push.remove("confirm");
            }
        }
        FieldUpdate::Unchanged => {}
    }
    match request.default_diff_exclusions {
        FieldUpdate::Update(extensions) => {
            if !document.contains_key("diff") {
                document["diff"] = Item::Table(Table::new());
            }
            let mut values = Array::new();
            for extension in extensions.extensions() {
                values.push(extension.as_str());
            }
            document["diff"]["exclude"] = Item::Value(Value::Array(values));
        }
        FieldUpdate::Clear => {
            if let Some(diff) = document.get_mut("diff").and_then(Item::as_table_mut) {
                diff.remove("exclude");
            }
        }
        FieldUpdate::Unchanged => {}
    }
    match request.projects {
        FieldUpdate::Update(projects) => {
            let mut tables = ArrayOfTables::new();
            for project in projects {
                let mut table = Table::new();
                table["name"] = toml_edit::value(project.name.to_string());
                table["excluded_from_push_all"] = toml_edit::value(project.excluded_from_push_all);
                let mut diff = Table::new();
                let mut values = Array::new();
                for extension in project.diff_exclusions.extensions() {
                    values.push(extension.as_str());
                }
                diff["exclude"] = Item::Value(Value::Array(values));
                table["diff"] = Item::Table(diff);
                tables.push(table);
            }
            document["projects"] = Item::ArrayOfTables(tables);
        }
        FieldUpdate::Clear => {
            document.remove("projects");
        }
        FieldUpdate::Unchanged => {}
    }
}

/// Resolve the config path from explicit sources, in precedence order. Pure.
fn config_path_from(
    env_override: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(path) = env_override {
        return Some(path);
    }
    if let Some(xdg) = xdg_config_home {
        return Some(xdg.join("git-tools").join("config.toml"));
    }
    home.map(|home| home.join(".config").join("git-tools").join("config.toml"))
}

fn config_path() -> Option<PathBuf> {
    let env_override = std::env::var_os("GIT_TOOLS_CONFIG").map(PathBuf::from);
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    config_path_from(env_override, xdg_config_home, home)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use gtl_application::settings::{remove_setting_key, set_setting_key};
    use gtl_models::paths::RepositoryRelativePath;
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn load_from_reads_theme_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "theme = \"light\"").unwrap();
        assert_eq!(
            load_from(Some(file.path())).unwrap().theme(),
            Some(Theme::Light)
        );
    }

    #[test]
    fn load_from_none_is_default() {
        assert_eq!(load_from(None).unwrap().theme(), None);
    }

    #[test]
    fn load_from_nonexistent_path_is_default() {
        assert_eq!(
            load_from(Some(Path::new("/no/such/git-tools/config.toml")))
                .unwrap()
                .theme(),
            None
        );
    }

    #[test]
    fn load_from_unreadable_path_is_an_error() {
        let directory = tempfile::tempdir().unwrap();

        assert!(matches!(
            load_from(Some(directory.path())),
            Err(UserSettingsLoadError::Read { path, .. }) if path == directory.path()
        ));
    }

    #[test]
    fn load_from_malformed_or_invalid_settings_is_an_error() {
        let file = NamedTempFile::new().unwrap();
        for raw in [
            "theme = {{{\n",
            "layout = \"diagonal\"\n",
            "[push]\nconfirm = \"yes\"\n",
            "[push]\nconfrm = false\n",
            "[diff]\nexclude = \"md\"\n",
            "[[projects]]\nexcluded_from_push_all = true\n",
            "[[projects]]\nname = \"repo\"\nexcluded_from_push_all = \"yes\"\n",
            "[[projects]]\nname = \"repo\"\ndiff = { exclude = \"md\" }\n",
        ] {
            std::fs::write(file.path(), raw).unwrap();
            assert!(matches!(
                load_from(Some(file.path())),
                Err(UserSettingsLoadError::InvalidConfiguration { .. })
            ));
        }
    }

    #[test]
    fn strict_project_settings_map_defaults_overrides_and_push_exclusions() {
        let settings = parse_settings(
            Path::new("config.toml"),
            r#"
[diff]
exclude = ["md", "lock"]

[[projects]]
name = "git-tools"
excluded_from_push_all = true
diff = { exclude = ["js"] }

[[projects]]
name = "sample_project"
diff = { exclude = [] }
"#,
        )
        .unwrap();
        let git_tools = ProjectName::try_from("git-tools").unwrap();
        let sample_project = ProjectName::try_from("sample_project").unwrap();
        let unconfigured = ProjectName::try_from("unconfigured").unwrap();

        assert!(settings.push_all_exclusions().contains(&git_tools));
        assert!(!settings.push_all_exclusions().contains(&sample_project));
        assert_eq!(
            settings
                .diff_exclusions()
                .for_project_or_default(&git_tools)
                .extensions(),
            ["js"]
        );
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(&sample_project)
                .is_empty()
        );
        assert_eq!(
            settings
                .diff_exclusions()
                .for_project_or_default(&unconfigured)
                .extensions(),
            ["lock", "md"]
        );
    }

    #[test]
    fn duplicate_project_settings_are_invalid() {
        let error = parse_settings(
            Path::new("config.toml"),
            r#"
[[projects]]
name = "git-tools"

[[projects]]
name = "git-tools"
excluded_from_push_all = true
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("duplicate project name `git-tools`")
        );
    }

    #[test]
    fn map_based_diff_exclusions_are_invalid() {
        let error = parse_settings(
            Path::new("config.toml"),
            "[diff.exclude]\ndefaults = [\"md\"]\ngit-tools = [\"js\"]\n",
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("`diff.exclude` must be an array of strings")
        );
    }

    #[test]
    fn load_from_non_utf8_settings_is_an_invalid_configuration_error() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0xff, 0xfe]).unwrap();

        assert!(matches!(
            load_from(Some(file.path())),
            Err(UserSettingsLoadError::InvalidConfiguration { path, .. })
                if path == file.path()
        ));
    }

    #[test]
    fn checked_in_example_maps_every_validated_setting() {
        let settings = parse_settings(
            Path::new("config.example.toml"),
            include_str!("../../../config/local/config.example.toml"),
        )
        .unwrap();

        assert_eq!(settings.theme(), Some(Theme::Dark));
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        let git_tools = ProjectName::try_from("git-tools").unwrap();
        assert!(settings.push_all_exclusions().contains(&git_tools));
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(&git_tools)
                .matches(&RepositoryRelativePath::try_new("frontend.js".into()).unwrap(),)
        );
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(&ProjectName::try_from("unconfigured").unwrap(),)
                .matches(&RepositoryRelativePath::try_new("README.md".into()).unwrap(),)
        );
    }

    #[test]
    fn toml_settings_store_reads_fresh_snapshot() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "theme = \"light\"").unwrap();
        let store = TomlSettingsStore::new(Some(file.path().to_path_buf()));

        assert_eq!(store.load().unwrap().theme(), Some(Theme::Light));

        std::fs::write(file.path(), "theme = \"hearth\"").unwrap();
        assert_eq!(store.load().unwrap().theme(), Some(Theme::Hearth));
    }

    #[test]
    fn set_and_remove_operations_affect_load_and_notify_the_viewer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# settings\ntheme = \"dark\"\nlayout = \"split\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let viewer = gtl_application::viewer::ViewerState::new();

        let set = set_setting_key::execute(
            gtl_models::settings::SettingKeyValue::Theme(Theme::Light),
            &mut store,
            &viewer,
        )
        .unwrap();
        let removed = remove_setting_key::execute(
            gtl_models::settings::SettingKey::Layout,
            &mut store,
            &viewer,
        )
        .unwrap();

        assert!(!set.viewer_rows_changed);
        assert!(removed.viewer_rows_changed);
        assert_eq!(
            viewer.version().unwrap(),
            gtl_models::viewer::ViewerVersion::new(2)
        );
        assert_eq!(store.load().unwrap().theme(), Some(Theme::Light));
        assert_eq!(
            store.load().unwrap().viewer_render_options(),
            gtl_models::viewer::RenderOptions::DEFAULT
        );
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("# settings")
        );
    }

    #[test]
    fn batch_edit_applies_every_field_atomically_and_clear_restores_defaults() {
        use gtl_application::settings::edit_settings::{
            EditSettingsRequest, FieldUpdate, ProjectSettingsUpdate,
        };
        use gtl_models::diffs::ExcludedExtensions;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let request = EditSettingsRequest {
            theme: FieldUpdate::Clear,
            layout: FieldUpdate::Update(DiffLayout::Split),
            density: FieldUpdate::Update(DiffDensity::Full),
            push_confirmation_required: FieldUpdate::Update(false),
            default_diff_exclusions: FieldUpdate::Update(ExcludedExtensions::new([".MD"])),
            projects: FieldUpdate::Update(vec![ProjectSettingsUpdate {
                name: ProjectName::try_from("git-tools").unwrap(),
                excluded_from_push_all: true,
                diff_exclusions: ExcludedExtensions::new(["lock"]),
            }]),
        };

        assert_eq!(
            store.edit_settings(request.clone()).unwrap(),
            UserSettingsEditOutcome::Changed
        );
        assert_eq!(
            store.edit_settings(request).unwrap(),
            UserSettingsEditOutcome::Unchanged
        );
        let settings = store.load().unwrap();
        assert_eq!(settings.theme(), None);
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert!(!settings.push_confirmation_required());
        assert_eq!(
            settings.diff_exclusions().default_exclusions().extensions(),
            ["md"]
        );
        assert!(
            settings
                .push_all_exclusions()
                .contains(&ProjectName::try_from("git-tools").unwrap())
        );
    }

    #[test]
    fn remove_operation_rejects_a_non_string_without_changing_the_target() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = b"layout = [\"split\"]\n";
        std::fs::write(&path, raw).unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let viewer = gtl_application::viewer::ViewerState::new();

        let error = remove_setting_key::execute(
            gtl_models::settings::SettingKey::Layout,
            &mut store,
            &viewer,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            remove_setting_key::RemoveSettingKeyError::InvalidValueShape { key }
                if key == gtl_models::settings::SettingKey::Layout
        ));
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }

    #[test]
    fn set_operation_reports_invalid_configuration_without_changing_malformed_toml() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = b"theme = {{{\n";
        std::fs::write(&path, raw).unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let viewer = gtl_application::viewer::ViewerState::new();

        let error = set_setting_key::execute(
            gtl_models::settings::SettingKeyValue::Theme(Theme::Light),
            &mut store,
            &viewer,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            set_setting_key::SetSettingKeyError::Settings(
                UserSettingsEditError::InvalidConfiguration { path: error_path, .. }
            ) if error_path == path
        ));
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }

    #[test]
    fn config_path_env_override_wins() {
        let path = config_path_from(
            Some(PathBuf::from("/explicit/config.toml")),
            Some(PathBuf::from("/xdg")),
            Some(PathBuf::from("/home/user")),
        );
        assert_eq!(path, Some(PathBuf::from("/explicit/config.toml")));
    }

    #[test]
    fn config_path_xdg_wins_over_home() {
        let path = config_path_from(
            None,
            Some(PathBuf::from("/xdg")),
            Some(PathBuf::from("/home/user")),
        );
        assert_eq!(path, Some(PathBuf::from("/xdg/git-tools/config.toml")));
    }

    #[test]
    fn config_path_falls_back_to_home() {
        let path = config_path_from(None, None, Some(PathBuf::from("/home/user")));
        assert_eq!(
            path,
            Some(PathBuf::from("/home/user/.config/git-tools/config.toml"))
        );
    }

    #[test]
    fn config_path_all_none_is_none() {
        assert_eq!(config_path_from(None, None, None), None);
    }
}
