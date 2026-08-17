//! TOML-backed user settings shared by the CLI, daemon, and desktop process roots.

mod string_editor;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use gtl_application::ports::{UserSettingsEditError, UserSettingsLoadError, UserSettingsStore};
use gtl_models::{
    diffs::DiffExclusions,
    paths::ProjectName,
    settings::UserSettings,
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};
use gtl_wire::settings::{RawSettingValue, UserSettingsDocument};

fn default_settings() -> UserSettings {
    UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT,
        DiffExclusions::default(),
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

fn exclusions(
    path: &Path,
    values: Option<BTreeMap<String, RawSettingValue>>,
) -> Result<DiffExclusions, UserSettingsLoadError> {
    let mut resolved = BTreeMap::new();
    for (project, value) in values.unwrap_or_default() {
        let RawSettingValue::Array(values) = value else {
            return Err(invalid_configuration(
                path,
                format!("`diff.exclude.{project}` must be an array of strings"),
            ));
        };
        let extensions = values
            .into_iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    invalid_configuration(
                        path,
                        format!("`diff.exclude.{project}` must contain only strings"),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let project_name = ProjectName::try_new(project.clone()).map_err(|error| {
            invalid_configuration(
                path,
                format!("`diff.exclude.{project}` has an invalid project name: {error}"),
            )
        })?;
        resolved.insert(project_name, extensions);
    }
    Ok(DiffExclusions::new(resolved, None))
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
    let push_confirmation_required = document
        .push
        .and_then(|push| push.confirm)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid_configuration(path, "`push.confirm` must be a boolean"))
        })
        .transpose()?
        .unwrap_or(UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT);
    let diff_exclusions = exclusions(path, document.diff.and_then(|diff| diff.exclude))?;

    Ok(UserSettings::new(
        theme,
        RenderOptions::new(layout, density),
        push_confirmation_required,
        diff_exclusions,
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
    pub const fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// Resolves the production configuration path from the process environment.
    pub fn from_environment() -> Self {
        Self::new(config_path())
    }

    /// Returns the resolved configuration path when one is available.
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
    ) -> Result<Option<String>, UserSettingsEditError> {
        let key = mutation.key();
        let value_new = mutation.value();
        string_editor::edit(
            self.required_path()?,
            key.as_str(),
            string_editor::StringEdit::Set(&value_new),
        )
        .map(|outcome| outcome.value_old)
    }

    fn remove_key(
        &mut self,
        key: gtl_models::settings::SettingKey,
    ) -> Result<Option<String>, UserSettingsEditError> {
        string_editor::edit(
            self.required_path()?,
            key.as_str(),
            string_editor::StringEdit::Remove,
        )
        .map(|outcome| outcome.value_old)
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
        let mut file = NamedTempFile::new().expect("create temp config");
        write!(file, "theme = \"light\"").expect("write temp config");
        assert_eq!(
            load_from(Some(file.path()))
                .expect("valid settings")
                .theme(),
            Some(Theme::Light)
        );
    }

    #[test]
    fn load_from_none_is_default() {
        assert_eq!(load_from(None).expect("default settings").theme(), None);
    }

    #[test]
    fn load_from_nonexistent_path_is_default() {
        assert_eq!(
            load_from(Some(Path::new("/no/such/git-tools/config.toml")))
                .expect("missing settings use defaults")
                .theme(),
            None
        );
    }

    #[test]
    fn load_from_unreadable_path_is_an_error() {
        let directory = tempfile::tempdir().expect("create temp directory");

        assert!(matches!(
            load_from(Some(directory.path())),
            Err(UserSettingsLoadError::Read { path, .. }) if path == directory.path()
        ));
    }

    #[test]
    fn load_from_malformed_or_invalid_settings_is_an_error() {
        let file = NamedTempFile::new().expect("create temp config");
        for raw in [
            "theme = {{{\n",
            "layout = \"diagonal\"\n",
            "[push]\nconfirm = \"yes\"\n",
            "[push]\nconfrm = false\n",
        ] {
            std::fs::write(file.path(), raw).expect("write invalid config");
            assert!(matches!(
                load_from(Some(file.path())),
                Err(UserSettingsLoadError::InvalidConfiguration { .. })
            ));
        }
    }

    #[test]
    fn load_from_non_utf8_settings_is_an_invalid_configuration_error() {
        let file = NamedTempFile::new().expect("create temp config");
        std::fs::write(file.path(), [0xff, 0xfe]).expect("write non-UTF-8 config");

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
        .expect("checked-in settings example is valid");

        assert_eq!(settings.theme(), Some(Theme::Dark));
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(
                    &ProjectName::try_from("git-tools").expect("valid fixture project name"),
                )
                .matches(
                    &RepositoryRelativePath::try_new("frontend.js".into())
                        .expect("valid fixture repository-relative path"),
                )
        );
        assert!(
            settings
                .diff_exclusions()
                .for_project_or_default(
                    &ProjectName::try_from("unconfigured").expect("valid fixture project name"),
                )
                .matches(
                    &RepositoryRelativePath::try_new("README.md".into())
                        .expect("valid fixture repository-relative path"),
                )
        );
    }

    #[test]
    fn toml_settings_store_reads_fresh_snapshot() {
        let file = NamedTempFile::new().expect("create temp config");
        std::fs::write(file.path(), "theme = \"light\"").expect("write first config");
        let store = TomlSettingsStore::new(Some(file.path().to_path_buf()));

        assert_eq!(
            store.load().expect("first settings").theme(),
            Some(Theme::Light)
        );

        std::fs::write(file.path(), "theme = \"hearth\"").expect("write second config");
        assert_eq!(
            store.load().expect("second settings").theme(),
            Some(Theme::Hearth)
        );
    }

    #[test]
    fn set_and_remove_operations_return_previous_values_and_affect_load() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# settings\ntheme = \"dark\"\nlayout = \"split\"\n")
            .expect("seed config");
        let mut store = TomlSettingsStore::new(Some(path.clone()));

        let set = set_setting_key::execute(
            set_setting_key::SetSettingKey {
                mutation: gtl_models::settings::SettingKeyValue::Theme(Theme::Light),
            },
            &mut store,
        )
        .expect("set theme");
        let removed = remove_setting_key::execute(
            remove_setting_key::RemoveSettingKey {
                key: gtl_models::settings::SettingKey::Layout,
            },
            &mut store,
        )
        .expect("remove layout");

        assert_eq!(set.value_old.as_deref(), Some("dark"));
        assert_eq!(removed.value_old.as_deref(), Some("split"));
        assert_eq!(
            store.load().expect("updated settings").theme(),
            Some(Theme::Light)
        );
        assert_eq!(
            store
                .load()
                .expect("updated settings")
                .viewer_render_options(),
            gtl_models::viewer::RenderOptions::DEFAULT
        );
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("# settings")
        );
    }

    #[test]
    fn remove_operation_rejects_a_non_string_without_changing_the_target() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let raw = b"layout = [\"split\"]\n";
        std::fs::write(&path, raw).expect("seed config");
        let mut store = TomlSettingsStore::new(Some(path.clone()));

        let error = remove_setting_key::execute(
            remove_setting_key::RemoveSettingKey {
                key: gtl_models::settings::SettingKey::Layout,
            },
            &mut store,
        )
        .expect_err("non-string layout is rejected");

        assert!(matches!(
            error,
            remove_setting_key::RemoveSettingKeyError::InvalidValueShape { key }
                if key == gtl_models::settings::SettingKey::Layout
        ));
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }

    #[test]
    fn set_operation_reports_invalid_configuration_without_changing_malformed_toml() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let raw = b"theme = {{{\n";
        std::fs::write(&path, raw).expect("seed config");
        let mut store = TomlSettingsStore::new(Some(path.clone()));

        let error = set_setting_key::execute(
            set_setting_key::SetSettingKey {
                mutation: gtl_models::settings::SettingKeyValue::Theme(Theme::Light),
            },
            &mut store,
        )
        .expect_err("malformed TOML is rejected");
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
