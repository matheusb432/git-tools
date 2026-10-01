//! TOML-backed user settings used by the server process root.

mod document;
mod file_editor;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use gtl_application::{
    ports::{
        UserSettingsConfigurationError, UserSettingsEditError, UserSettingsEditOutcome,
        UserSettingsEditor, UserSettingsLoadError, UserSettingsReader,
    },
    settings::UserSettingsPatch,
};
use gtl_models::settings::{ProjectsPreferences, UserSettings, UserSettingsRevision};
use parking_lot::Mutex;

use self::document::UserSettingsDocument;

fn settings_document(
    path: &Path,
    bytes: Vec<u8>,
) -> Result<UserSettingsDocument, UserSettingsConfigurationError> {
    UserSettingsDocument::parse(bytes).map_err(|source| {
        UserSettingsConfigurationError::new(path.to_path_buf(), anyhow::Error::new(source))
    })
}

#[derive(Debug, Default)]
struct UserSettingsCache {
    bytes: Vec<u8>,
    viewer_settings: (UserSettings, ProjectsPreferences),
}

/// TOML-backed user settings with a parsed cache shared by every clone.
#[derive(Debug, Clone)]
pub struct TomlSettingsStore {
    path: Option<PathBuf>,
    cache: Arc<Mutex<UserSettingsCache>>,
}

impl TomlSettingsStore {
    /// Creates a store for an explicit path, or an unresolved production path.
    #[must_use]
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cache: Arc::new(Mutex::new(UserSettingsCache::default())),
        }
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

    /// Rereads the file on every call and reuses settings when its bytes match the cache.
    pub fn load_viewer_settings(
        &self,
    ) -> Result<(UserSettings, ProjectsPreferences), UserSettingsLoadError> {
        Ok(self.load_viewer_settings_with_revision()?.0)
    }

    /// Loads the viewer settings and the exact serialized document revision they came from.
    pub fn load_viewer_settings_with_revision(
        &self,
    ) -> Result<((UserSettings, ProjectsPreferences), UserSettingsRevision), UserSettingsLoadError>
    {
        let mut cache = self.cache.lock();
        if let Some(path) = self.path() {
            let bytes = file_editor::read_document_bytes(path)?;
            if cache.bytes != bytes {
                let viewer_settings =
                    settings_document(path, bytes.clone())?.into_viewer_settings();
                *cache = UserSettingsCache {
                    bytes,
                    viewer_settings,
                };
            }
        }
        Ok((
            cache.viewer_settings.clone(),
            file_editor::revision(&cache.bytes),
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
        Ok(self.load_viewer_settings()?.0)
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

impl gtl_application::ports::UserSettingsRecovery for TomlSettingsStore {
    fn inspect(
        &self,
    ) -> Result<gtl_application::ports::UserSettingsRecoveryState, UserSettingsLoadError> {
        let path = self.required_path()?;
        let bytes = file_editor::read_document_bytes(path)?;
        let revision = file_editor::revision(&bytes).to_string();
        let diagnostic = UserSettingsDocument::parse(bytes)
            .err()
            .map(|error| error.to_string());
        Ok(gtl_application::ports::UserSettingsRecoveryState {
            configuration_path: path.to_path_buf(),
            diagnostic,
            revision,
        })
    }

    fn reset_invalid(
        &mut self,
        revision: &str,
        timestamp: &gtl_models::timestamps::MachineTimestamp,
    ) -> Result<PathBuf, UserSettingsEditError> {
        file_editor::reset_invalid(self.required_path()?, revision, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use gtl_application::settings::{
        UserSettingsFieldUpdate, UserSettingsPatch, remove_setting_key, set_setting_key,
    };
    use gtl_models::{
        paths::ProjectName,
        settings::{DiffFilesSort, ProjectsPageSize, ProjectsSort},
        viewer::{
            DiffDensity, DiffLayout, RenderOptions, Theme, ViewerKeybinding,
            ViewerKeybindingAction, ViewerKeybindings,
        },
    };
    use tempfile::NamedTempFile;

    use super::*;

    fn parse_settings(
        path: &Path,
        raw: &str,
    ) -> Result<UserSettings, UserSettingsConfigurationError> {
        settings_document(path, raw.as_bytes().to_vec())
            .map(|document| document.into_viewer_settings().0)
    }

    #[test]
    fn removed_saved_themes_fall_back_without_rewriting_other_settings() {
        for theme in ["verdant", "noir", "light", "hearth"] {
            let mut file = NamedTempFile::new().unwrap();
            let raw = format!("# keep\ntheme = \"{theme}\"\nwrap_lines = true\n");
            file.write_all(raw.as_bytes()).unwrap();
            let settings = TomlSettingsStore::new(Some(file.path().to_path_buf()))
                .load()
                .unwrap();
            assert_eq!(settings.theme(), Some(Theme::Dark));
            assert_eq!(std::fs::read_to_string(file.path()).unwrap(), raw);
        }
    }

    #[test]
    fn load_reads_theme_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "theme = \"glacier\"").unwrap();
        assert_eq!(
            TomlSettingsStore::new(Some(file.path().to_path_buf()))
                .load()
                .unwrap()
                .theme(),
            Some(Theme::Glacier)
        );
    }

    #[test]
    fn load_none_is_default() {
        let settings = TomlSettingsStore::new(None).load().unwrap();

        assert_eq!(settings.theme(), None);
        assert!(settings.copy_with_line_context());
        assert_eq!(settings.viewer_keybindings(), ViewerKeybindings::default());
    }

    #[test]
    fn load_nonexistent_path_is_default() {
        assert_eq!(
            TomlSettingsStore::new(Some(PathBuf::from("/no/such/git-tools/config.toml")))
                .load()
                .unwrap()
                .theme(),
            None
        );
    }

    #[test]
    fn load_unreadable_path_is_an_error() {
        let directory = tempfile::tempdir().unwrap();

        assert!(matches!(
            TomlSettingsStore::new(Some(directory.path().to_path_buf())).load(),
            Err(UserSettingsLoadError::Adapter(_))
        ));
    }

    #[test]
    fn load_malformed_or_invalid_settings_is_an_error() {
        let file = NamedTempFile::new().unwrap();
        for raw in [
            "theme = {{{\n",
            "layout = \"diagonal\"\n",
            "[push]\nconfirm = \"yes\"\n",
            "[push]\nconfrm = false\n",
            "[diff]\nexclude = [\"md\"]\n",
            "[keybindings]\nsearch_files = false\n",
            "[[projects]]\nexcluded_from_push_all = true\n",
            "[[projects]]\nname = \"repo\"\nexcluded_from_push_all = \"yes\"\n",
            "[[projects]]\nname = \"repo\"\ndiff = { exclude = [\"md\"] }\n",
        ] {
            std::fs::write(file.path(), raw).unwrap();
            assert!(matches!(
                TomlSettingsStore::new(Some(file.path().to_path_buf())).load(),
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
        assert_eq!(
            settings.viewer_keybindings()[ViewerKeybindingAction::PushDiff].to_string(),
            "ctrl+enter"
        );
    }

    #[test]
    fn push_keybinding_overrides_are_validated_with_the_other_actions() {
        let path = Path::new("config.toml");
        let settings =
            parse_settings(path, "[keybindings]\npush_diff = \"Alt + Enter\"\n").unwrap();
        assert_eq!(
            settings.viewer_keybindings()[ViewerKeybindingAction::PushDiff].to_string(),
            "alt+enter"
        );
        for raw in [
            "[keybindings]\npush_diff = \"cmd+enter\"\n",
            "[keybindings]\npush_diff = \"ctrl+p\"\n",
        ] {
            let message = parse_settings(path, raw).unwrap_err().to_string();
            assert!(message.contains("`keybindings.push_diff`"));
        }
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
    fn strict_project_settings_map_push_exclusions() {
        let settings = parse_settings(
            Path::new("config.toml"),
            r#"
[[projects]]
name = "git-tools"
excluded_from_push_all = true

[[projects]]
name = "example-project"
"#,
        )
        .unwrap();
        let git_tools = ProjectName::try_from("git-tools").unwrap();
        let example_project = ProjectName::try_from("example-project").unwrap();

        assert!(settings.push_all_exclusions().contains(&git_tools));
        assert!(!settings.push_all_exclusions().contains(&example_project));
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
    fn load_non_utf8_settings_is_an_invalid_configuration_error() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0xff, 0xfe]).unwrap();

        assert!(matches!(
            TomlSettingsStore::new(Some(file.path().to_path_buf())).load(),
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
    }

    #[test]
    fn toml_settings_store_reads_fresh_snapshot() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "theme = \"glacier\"").unwrap();
        let store = TomlSettingsStore::new(Some(file.path().to_path_buf()));

        assert_eq!(store.load().unwrap().theme(), Some(Theme::Glacier));

        std::fs::write(file.path(), "theme = \"mirage\"").unwrap();
        assert_eq!(store.load().unwrap().theme(), Some(Theme::Mirage));
    }

    #[test]
    fn set_and_remove_operations_affect_load_and_notify_the_viewer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# settings\ntheme = \"dark\"\nlayout = \"split\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let viewer = gtl_application::viewer::ViewerState::new();

        let set = set_setting_key::execute(
            gtl_models::settings::SettingKeyValue::Theme(Theme::Glacier),
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
        assert_eq!(store.load().unwrap().theme(), Some(Theme::Glacier));
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
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let settings_patch = UserSettingsPatch {
            keybindings: UserSettingsFieldUpdate::Unchanged,
            ui_scale_percent: UserSettingsFieldUpdate::Update(
                gtl_models::settings::ViewerScalePercent::try_new(200).unwrap(),
            ),
            reduce_motion: UserSettingsFieldUpdate::Update(true),
            language: UserSettingsFieldUpdate::Update(gtl_models::settings::ViewerLanguage::PtBr),
            date_format: UserSettingsFieldUpdate::Update(
                gtl_models::settings::ViewerDateFormat::Relative,
            ),
            expected_revision: None,
            focus_window_on_diff: UserSettingsFieldUpdate::Update(false),
            files_sidebar_visible: UserSettingsFieldUpdate::Update(false),
            commits_sidebar_visible: UserSettingsFieldUpdate::Update(true),
            wrap_lines: UserSettingsFieldUpdate::Update(false),
            copy_with_line_context: UserSettingsFieldUpdate::Update(false),
            diff_files_sort: UserSettingsFieldUpdate::Update(DiffFilesSort::Changes),
            projects_sort: UserSettingsFieldUpdate::Update(ProjectsSort::Name),
            projects_page_size: UserSettingsFieldUpdate::Update(ProjectsPageSize::default()),
            theme: UserSettingsFieldUpdate::Clear,
            layout: UserSettingsFieldUpdate::Update(DiffLayout::Split),
            density: UserSettingsFieldUpdate::Update(DiffDensity::Full),
            push_confirmation_required: UserSettingsFieldUpdate::Update(false),
            viewer_push_confirmation_required: UserSettingsFieldUpdate::Update(false),
            viewer_push_no_confirmation_projects: UserSettingsFieldUpdate::Unchanged,
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
        assert_eq!(settings.accessibility().ui_scale_percent.into_inner(), 200);
        assert!(settings.accessibility().reduce_motion);
        assert_eq!(
            settings.language(),
            gtl_models::settings::ViewerLanguage::PtBr
        );
        assert_eq!(
            settings.date_format(),
            gtl_models::settings::ViewerDateFormat::Relative
        );
        assert_eq!(settings.theme(), None);
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full)
        );
        assert!(!settings.copy_with_line_context());
        assert_eq!(settings.diff_files_sort(), DiffFilesSort::Changes);
        assert!(!settings.push_confirmation_required());
    }

    #[test]
    fn accessibility_settings_persist_reset_and_reject_invalid_documents() {
        use gtl_models::settings::{ViewerAccessibility, ViewerScalePercent};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(
            store.load().unwrap().accessibility(),
            ViewerAccessibility::default()
        );
        store
            .edit(UserSettingsPatch {
                ui_scale_percent: UserSettingsFieldUpdate::Update(
                    ViewerScalePercent::try_new(300).unwrap(),
                ),
                reduce_motion: UserSettingsFieldUpdate::Update(true),
                ..Default::default()
            })
            .unwrap();
        let reloaded = TomlSettingsStore::new(Some(path.clone())).load().unwrap();
        assert_eq!(reloaded.accessibility().ui_scale_percent.into_inner(), 300);
        assert!(reloaded.accessibility().reduce_motion);
        store
            .edit(UserSettingsPatch {
                reduce_motion: UserSettingsFieldUpdate::Update(false),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store
                .load()
                .unwrap()
                .accessibility()
                .ui_scale_percent
                .into_inner(),
            300
        );
        assert!(!store.load().unwrap().accessibility().reduce_motion);
        store
            .edit(UserSettingsPatch {
                ui_scale_percent: UserSettingsFieldUpdate::Clear,
                reduce_motion: UserSettingsFieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store.load().unwrap().accessibility(),
            ViewerAccessibility::default()
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# retained\ntheme = \"dark\"\n"
        );
        for invalid in [
            "ui_scale_percent = 0",
            "ui_scale_percent = 301",
            "ui_scale_percent = 126",
            "ui_scale_percent = -100",
            "ui_scale_percent = '200'",
            "reduce_motion = 'true'",
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(
                TomlSettingsStore::new(Some(path.clone())).load().is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn language_persists_its_tag_clears_to_english_and_rejects_unknown_tags() {
        use gtl_models::settings::ViewerLanguage;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(store.load().unwrap().language(), ViewerLanguage::EnUs);
        store
            .edit(UserSettingsPatch {
                language: UserSettingsFieldUpdate::Update(ViewerLanguage::PtBr),
                ..Default::default()
            })
            .unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("language = \"pt-BR\"")
        );
        let reloaded = TomlSettingsStore::new(Some(path.clone())).load().unwrap();
        assert_eq!(reloaded.language(), ViewerLanguage::PtBr);
        store
            .edit(UserSettingsPatch {
                language: UserSettingsFieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(store.load().unwrap().language(), ViewerLanguage::EnUs);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# retained\ntheme = \"dark\"\n"
        );
        for invalid in ["language = 'pt-br'", "language = 'fr'", "language = 1"] {
            std::fs::write(&path, invalid).unwrap();
            assert!(
                TomlSettingsStore::new(Some(path.clone())).load().is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn date_format_persists_its_token_clears_to_iso_and_rejects_unknown_tokens() {
        use gtl_models::settings::ViewerDateFormat;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(store.load().unwrap().date_format(), ViewerDateFormat::Iso);
        store
            .edit(UserSettingsPatch {
                date_format: UserSettingsFieldUpdate::Update(ViewerDateFormat::DayFirst),
                ..Default::default()
            })
            .unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("date_format = \"day_first\"")
        );
        let reloaded = TomlSettingsStore::new(Some(path.clone())).load().unwrap();
        assert_eq!(reloaded.date_format(), ViewerDateFormat::DayFirst);
        store
            .edit(UserSettingsPatch {
                date_format: UserSettingsFieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(store.load().unwrap().date_format(), ViewerDateFormat::Iso);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# retained\ntheme = \"dark\"\n"
        );
        for invalid in [
            "date_format = 'Relative'",
            "date_format = 'locale'",
            "date_format = 1",
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(
                TomlSettingsStore::new(Some(path.clone())).load().is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn diff_window_focus_defaults_on_and_round_trips_false_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert!(store.load().unwrap().focus_window_on_diff());
        for (update, expected) in [
            (UserSettingsFieldUpdate::Update(false), false),
            (UserSettingsFieldUpdate::Unchanged, false),
            (UserSettingsFieldUpdate::Update(true), true),
            (UserSettingsFieldUpdate::Update(false), false),
            (UserSettingsFieldUpdate::Clear, true),
        ] {
            store
                .edit(UserSettingsPatch {
                    focus_window_on_diff: update,
                    ..UserSettingsPatch::default()
                })
                .unwrap();
            assert_eq!(store.load().unwrap().focus_window_on_diff(), expected);
            assert_eq!(store.load().unwrap().theme(), Some(Theme::Dark));
            assert!(
                std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("# retained")
            );
        }
        for invalid in ["\"false\"", "0", "[]"] {
            std::fs::write(&path, format!("focus_window_on_diff = {invalid}\n")).unwrap();
            assert!(store.load().is_err());
        }
    }

    #[test]
    fn sidebar_edits_preserve_each_other_and_clear_restores_visibility() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# retained\ntheme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path));
        assert_eq!(
            store.load().unwrap().sidebar_visibility(),
            gtl_models::viewer::ViewerSidebarVisibility {
                files: true,
                commits: true
            }
        );
        store
            .edit(UserSettingsPatch {
                files_sidebar_visible: UserSettingsFieldUpdate::Update(false),
                ..UserSettingsPatch::default()
            })
            .unwrap();
        store
            .edit(UserSettingsPatch {
                commits_sidebar_visible: UserSettingsFieldUpdate::Update(false),
                ..UserSettingsPatch::default()
            })
            .unwrap();
        assert_eq!(
            store.load().unwrap().sidebar_visibility(),
            gtl_models::viewer::ViewerSidebarVisibility {
                files: false,
                commits: false
            }
        );
        store
            .edit(UserSettingsPatch {
                files_sidebar_visible: UserSettingsFieldUpdate::Clear,
                ..UserSettingsPatch::default()
            })
            .unwrap();
        let settings = store.load().unwrap();
        assert_eq!(
            settings.sidebar_visibility(),
            gtl_models::viewer::ViewerSidebarVisibility {
                files: true,
                commits: false
            }
        );
        assert_eq!(settings.theme(), Some(Theme::Dark));
    }

    #[test]
    fn line_wrapping_defaults_off_and_round_trips_explicit_false_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "theme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert!(!store.load().unwrap().viewer_render_options().wrap_lines());
        for (update, expected) in [
            (UserSettingsFieldUpdate::Update(true), true),
            (UserSettingsFieldUpdate::Update(false), false),
            (UserSettingsFieldUpdate::Update(true), true),
            (UserSettingsFieldUpdate::Clear, false),
        ] {
            store
                .edit(UserSettingsPatch {
                    wrap_lines: update,
                    ..Default::default()
                })
                .unwrap();
            let settings = store.load().unwrap();
            assert_eq!(settings.viewer_render_options().wrap_lines(), expected);
            assert_eq!(
                settings
                    .viewer_render_options()
                    .with_layout(DiffLayout::Split)
                    .with_density(DiffDensity::Full)
                    .wrap_lines(),
                expected
            );
            assert_eq!(settings.theme(), Some(Theme::Dark));
        }
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("wrap_lines")
        );
        std::fs::write(&path, "wrap_lines = \"true\"\n").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn copy_with_line_context_defaults_on_and_round_trips_explicit_values_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "theme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert!(store.load().unwrap().copy_with_line_context());
        for (update, expected) in [
            (UserSettingsFieldUpdate::Update(false), false),
            (UserSettingsFieldUpdate::Update(true), true),
            (UserSettingsFieldUpdate::Update(false), false),
            (UserSettingsFieldUpdate::Clear, true),
        ] {
            store
                .edit(UserSettingsPatch {
                    copy_with_line_context: update,
                    ..Default::default()
                })
                .unwrap();
            let settings = store.load().unwrap();
            assert_eq!(settings.copy_with_line_context(), expected);
            assert_eq!(settings.theme(), Some(Theme::Dark));
        }
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("copy_with_line_context")
        );
        std::fs::write(&path, "copy_with_line_context = \"false\"\n").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn diff_files_sort_defaults_to_path_and_round_trips_explicit_values_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "theme = \"dark\"\n").unwrap();
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(store.load().unwrap().diff_files_sort(), DiffFilesSort::Path);
        for (update, expected) in [
            (
                UserSettingsFieldUpdate::Update(DiffFilesSort::Changes),
                DiffFilesSort::Changes,
            ),
            (UserSettingsFieldUpdate::Clear, DiffFilesSort::Path),
        ] {
            store
                .edit(UserSettingsPatch {
                    diff_files_sort: update,
                    ..Default::default()
                })
                .unwrap();
            let settings = store.load().unwrap();
            assert_eq!(settings.diff_files_sort(), expected);
            assert_eq!(settings.theme(), Some(Theme::Dark));
        }
        std::fs::write(&path, "diff_files_sort = \"changes\"\n").unwrap();
        assert_eq!(
            store.load().unwrap().diff_files_sort(),
            DiffFilesSort::Changes
        );
        std::fs::write(&path, "diff_files_sort = \"size\"\n").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn projects_page_size_defaults_and_persists_without_changing_other_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        assert_eq!(
            store
                .load_viewer_settings()
                .unwrap()
                .1
                .page_size
                .into_inner(),
            15
        );
        for raw in [
            "0",
            "-1",
            "20",
            "4294967296",
            "15.5",
            "true",
            "[]",
            "{}",
            "\"invalid\"",
            "\"15\"",
        ] {
            std::fs::write(
                &path,
                format!("# retained\ntheme = \"mirage\"\nprojects_page_size = {raw}\n"),
            )
            .unwrap();
            assert_eq!(
                store
                    .load_viewer_settings()
                    .unwrap()
                    .1
                    .page_size
                    .into_inner(),
                15,
                "{raw}"
            );
            assert_eq!(
                store.load().unwrap().theme(),
                Some(gtl_models::viewer::Theme::Mirage)
            );
        }
        for value in [10, 15, 30] {
            store
                .edit(UserSettingsPatch {
                    projects_page_size: UserSettingsFieldUpdate::Update(
                        ProjectsPageSize::try_new(value).unwrap(),
                    ),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                store
                    .load_viewer_settings()
                    .unwrap()
                    .1
                    .page_size
                    .into_inner(),
                value
            );
            let saved = std::fs::read_to_string(&path).unwrap();
            assert!(saved.contains(&format!("projects_page_size = {value}")));
            assert!(saved.contains("# retained"));
            assert_eq!(
                store.load().unwrap().theme(),
                Some(gtl_models::viewer::Theme::Mirage)
            );
        }
        store
            .edit(UserSettingsPatch {
                projects_page_size: UserSettingsFieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store
                .load_viewer_settings()
                .unwrap()
                .1
                .page_size
                .into_inner(),
            15
        );
        assert!(
            !std::fs::read_to_string(path)
                .unwrap()
                .contains("projects_page_size")
        );
    }

    #[test]
    fn retired_projects_view_is_ignored_and_removed_on_the_next_settings_edit() {
        for view in ["grid", "table"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            let raw = format!(
                "# retained\ntheme = \"mirage\"\nprojects_view = \"{view}\"\nprojects_sort = \"name\"\nprojects_page_size = 30\n"
            );
            std::fs::write(&path, &raw).unwrap();
            let mut store = TomlSettingsStore::new(Some(path.clone()));
            let (settings, projects) = store.load_viewer_settings().unwrap();
            assert_eq!(settings.theme(), Some(Theme::Mirage));
            assert_eq!(projects.sort, ProjectsSort::Name);
            assert_eq!(projects.page_size.into_inner(), 30);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);

            store
                .edit(UserSettingsPatch {
                    projects_page_size: UserSettingsFieldUpdate::Update(ProjectsPageSize::default()),
                    ..Default::default()
                })
                .unwrap();
            let saved = std::fs::read_to_string(&path).unwrap();
            assert!(!saved.contains("projects_view"));
            assert!(saved.contains("# retained"));
            assert_eq!(store.load().unwrap(), settings);
            let projects = store.load_viewer_settings().unwrap().1;
            assert_eq!(projects.sort, ProjectsSort::Name);
            assert_eq!(projects.page_size, ProjectsPageSize::default());
        }
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
            gtl_models::settings::SettingKeyValue::Theme(Theme::Glacier),
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
