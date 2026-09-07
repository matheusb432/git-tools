//! TOML-backed user settings used by the server process root.

mod document;
mod file_editor;

use std::path::{Path, PathBuf};

use anyhow::Context;
use gtl_application::{
    ports::{
        UserSettingsConfigurationError, UserSettingsEditError, UserSettingsEditOutcome,
        UserSettingsEditor, UserSettingsLoadError, UserSettingsReader,
    },
    settings::UserSettingsPatch,
};
use gtl_models::{
    diffs::DiffExclusions,
    settings::{ProjectsViewMode, PushAllExclusions, UserSettings},
    viewer::{RenderOptions, ViewerKeybindings},
};

use self::document::UserSettingsDocument;

fn default_settings() -> UserSettings {
    UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        ViewerKeybindings::default(),
        UserSettings::PUSH_CONFIRMATION_REQUIRED_DEFAULT,
        DiffExclusions::default(),
        PushAllExclusions::default(),
    )
}

fn settings_document(
    path: &Path,
    bytes: Vec<u8>,
) -> Result<UserSettingsDocument, UserSettingsConfigurationError> {
    UserSettingsDocument::parse(bytes).map_err(|source| {
        let client_diagnostic = source.client_diagnostic();
        let error =
            UserSettingsConfigurationError::new(path.to_path_buf(), anyhow::Error::new(source));
        match client_diagnostic {
            Some(diagnostic) => error.with_client_diagnostic(diagnostic),
            None => error,
        }
    })
}

fn load_document(
    path: Option<&Path>,
) -> Result<Option<UserSettingsDocument>, UserSettingsLoadError> {
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read(path) {
        Ok(bytes) => settings_document(path, bytes)
            .map(Some)
            .map_err(UserSettingsLoadError::from),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::Error::new(error)
            .context(format!("read user settings {}", path.display()))
            .into()),
    }
}

fn load_from(path: Option<&Path>) -> Result<UserSettings, UserSettingsLoadError> {
    Ok(load_document(path)?.map_or_else(default_settings, UserSettingsDocument::into_settings))
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

    pub fn load_viewer_settings(
        &self,
    ) -> Result<(UserSettings, ProjectsViewMode), UserSettingsLoadError> {
        Ok(load_document(self.path.as_deref())?.map_or_else(
            || (default_settings(), ProjectsViewMode::default()),
            UserSettingsDocument::into_viewer_settings,
        ))
    }

    fn required_path(&self) -> anyhow::Result<&Path> {
        self.path.as_deref().context(
            "could not resolve a config path (no GIT_TOOLS_CONFIG, XDG_CONFIG_HOME, or HOME)",
        )
    }
}

impl UserSettingsReader for TomlSettingsStore {
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
        load_from(self.path.as_deref())
    }
}

impl UserSettingsEditor for TomlSettingsStore {
    fn edit(
        &mut self,
        patch: UserSettingsPatch,
    ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
        file_editor::edit(self.required_path()?, patch)
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

    use gtl_application::settings::{
        ProjectSettingsUpdate, ProjectSettingsUpdates, UserSettingsFieldUpdate, UserSettingsPatch,
        remove_setting_key, set_setting_key,
    };
    use gtl_models::{
        paths::{ProjectName, RepositoryRelativePath},
        viewer::{DiffDensity, DiffLayout, Theme, ViewerKeybinding, ViewerKeybindingAction},
    };
    use tempfile::NamedTempFile;

    use super::*;

    fn parse_settings(
        path: &Path,
        raw: &str,
    ) -> Result<UserSettings, UserSettingsConfigurationError> {
        settings_document(path, raw.as_bytes().to_vec()).map(UserSettingsDocument::into_settings)
    }

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
        let settings = load_from(None).unwrap();

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_keybindings(), ViewerKeybindings::default());
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
            Err(UserSettingsLoadError::Adapter(_))
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
            "[keybindings]\nsearch_files = false\n",
            "[[projects]]\nexcluded_from_push_all = true\n",
            "[[projects]]\nname = \"repo\"\nexcluded_from_push_all = \"yes\"\n",
            "[[projects]]\nname = \"repo\"\ndiff = { exclude = \"md\" }\n",
        ] {
            std::fs::write(file.path(), raw).unwrap();
            assert!(matches!(
                load_from(Some(file.path())),
                Err(UserSettingsLoadError::InvalidConfiguration(_))
            ));
        }
    }

    #[test]
    fn keybindings_are_parsed_canonically_and_missing_fields_use_defaults() {
        let settings = parse_settings(
            Path::new("config.toml"),
            r#"
[keybindings]
search_files = " shift + alt + k "
"#,
        )
        .unwrap();

        assert_eq!(
            settings.viewer_keybindings()[ViewerKeybindingAction::SearchFiles],
            "alt+shift+k".parse::<ViewerKeybinding>().unwrap()
        );
        assert_eq!(
            settings.viewer_keybindings()[ViewerKeybindingAction::SearchTextInAllFiles].to_string(),
            "ctrl+f"
        );
    }

    #[test]
    fn unsupported_and_conflicting_keybindings_name_the_relevant_fields() {
        let unsupported = parse_settings(
            Path::new("config.toml"),
            "[keybindings]\nsearch_files = \"Cmd+P\"\n",
        )
        .unwrap_err();
        assert!(
            unsupported
                .to_string()
                .contains("`keybindings.search_files` is invalid: unsupported token `Cmd`")
        );

        let conflict = parse_settings(
            Path::new("config.toml"),
            r#"
[keybindings]
search_files = "Ctrl+F"
search_text_in_all_files = "ctrl+f"
"#,
        )
        .unwrap_err();
        let message = conflict.to_string();
        assert!(message.contains("`keybindings.search_files`"));
        assert!(message.contains("`keybindings.search_text_in_all_files`"));
        assert!(message.contains("conflict"));
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
            Err(UserSettingsLoadError::InvalidConfiguration(configuration))
                if configuration.path() == file.path()
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
        assert_eq!(settings.viewer_keybindings(), ViewerKeybindings::default());
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
        use gtl_models::diffs::ExcludedExtensions;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let settings_patch = UserSettingsPatch {
            projects_view: UserSettingsFieldUpdate::Update(ProjectsViewMode::Table),
            theme: UserSettingsFieldUpdate::Clear,
            layout: UserSettingsFieldUpdate::Update(DiffLayout::Split),
            density: UserSettingsFieldUpdate::Update(DiffDensity::Full),
            push_confirmation_required: UserSettingsFieldUpdate::Update(false),
            default_diff_exclusions: UserSettingsFieldUpdate::Update(ExcludedExtensions::new([
                ".MD",
            ])),
            projects: UserSettingsFieldUpdate::Update(
                ProjectSettingsUpdates::try_new([ProjectSettingsUpdate {
                    name: ProjectName::try_from("git-tools").unwrap(),
                    excluded_from_push_all: true,
                    diff_exclusions: ExcludedExtensions::new(["lock"]),
                }])
                .unwrap(),
            ),
        };

        assert_eq!(
            store.edit(settings_patch.clone()).unwrap(),
            UserSettingsEditOutcome::Changed
        );
        assert_eq!(
            store.edit(settings_patch).unwrap(),
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
    fn projects_view_round_trips_and_clear_restores_grid_without_losing_other_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"hearth\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(
            store.load_viewer_settings().unwrap().1,
            ProjectsViewMode::Grid
        );
        let application_settings = store.load().unwrap();
        store
            .edit(UserSettingsPatch {
                projects_view: UserSettingsFieldUpdate::Update(ProjectsViewMode::Table),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store.load_viewer_settings().unwrap().1,
            ProjectsViewMode::Table
        );
        assert_eq!(store.load().unwrap(), application_settings);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("# retained")
        );
        store
            .edit(UserSettingsPatch {
                projects_view: UserSettingsFieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store.load_viewer_settings().unwrap().1,
            ProjectsViewMode::Grid
        );
        std::fs::write(&path, "projects_view = \"unknown\"\n").unwrap();
        assert!(matches!(
            store.load(),
            Err(UserSettingsLoadError::InvalidConfiguration(_))
        ));
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
            remove_setting_key::RemoveSettingKeyError::Settings(
                UserSettingsEditError::InvalidConfiguration(configuration)
            ) if configuration.path() == path
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
                UserSettingsEditError::InvalidConfiguration(configuration)
            ) if configuration.path() == path
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
