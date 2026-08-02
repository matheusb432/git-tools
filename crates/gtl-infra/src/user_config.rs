//! TOML-backed user settings shared by the CLI, daemon, and desktop process roots.

mod string_editor;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gtl_application::ports::{
    AppSettings, UserSettingsEditError, UserSettingsEditor, UserSettingsStore,
};

fn load_from(path: Option<&Path>) -> AppSettings {
    let Some(path) = path else {
        return AppSettings::default();
    };
    match std::fs::read_to_string(path) {
        Ok(raw) => gtl_application::settings::load::from_toml(&raw),
        Err(_) => AppSettings::default(),
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
    fn load(&self) -> AppSettings {
        load_from(self.path.as_deref())
    }
}

impl UserSettingsEditor for TomlSettingsStore {
    fn set_string(
        &self,
        key: &str,
        value_new: &str,
    ) -> Result<Option<String>, UserSettingsEditError> {
        string_editor::edit(
            self.required_path()?,
            key,
            string_editor::StringEdit::Set(value_new),
        )
        .map(|outcome| outcome.value_old)
    }

    fn remove_string(&self, key: &str) -> Result<Option<String>, UserSettingsEditError> {
        string_editor::edit(
            self.required_path()?,
            key,
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

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn load_from_reads_theme_from_file() {
        let mut file = NamedTempFile::new().expect("create temp config");
        write!(file, "theme = \"light\"").expect("write temp config");
        assert_eq!(load_from(Some(file.path())).theme(), Some("light"));
    }

    #[test]
    fn load_from_none_is_default() {
        assert_eq!(load_from(None).theme(), None);
    }

    #[test]
    fn load_from_nonexistent_path_is_default() {
        assert_eq!(
            load_from(Some(Path::new("/no/such/git-tools/config.toml"))).theme(),
            None
        );
    }

    #[test]
    fn load_from_unreadable_path_is_default() {
        let directory = tempfile::tempdir().expect("create temp directory");

        assert_eq!(load_from(Some(directory.path())), AppSettings::default());
    }

    #[test]
    fn toml_settings_store_reads_fresh_snapshot() {
        let file = NamedTempFile::new().expect("create temp config");
        std::fs::write(file.path(), "theme = \"light\"").expect("write first config");
        let store = TomlSettingsStore::new(Some(file.path().to_path_buf()));

        assert_eq!(store.load().theme(), Some("light"));

        std::fs::write(file.path(), "theme = \"hearth\"").expect("write second config");
        assert_eq!(store.load().theme(), Some("hearth"));
    }

    #[test]
    fn set_and_remove_operations_return_previous_values_and_affect_load() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "# settings\ntheme = \"dark\"\nlayout = \"split\"\n")
            .expect("seed config");
        let store = TomlSettingsStore::new(Some(path.clone()));

        let set = gtl_application::settings::set_key::execute(
            gtl_application::settings::set_key::SetSettingKey {
                key: "theme".into(),
                value_new: "light".into(),
            },
            &store,
        )
        .expect("set theme");
        let removed = gtl_application::settings::remove_key::execute(
            gtl_application::settings::remove_key::RemoveSettingKey {
                key: "layout".into(),
            },
            &store,
        )
        .expect("remove layout");

        assert_eq!(set.value_old.as_deref(), Some("dark"));
        assert_eq!(removed.value_old.as_deref(), Some("split"));
        assert_eq!(store.load().theme(), Some("light"));
        assert_eq!(
            store.load().viewer_render_options(),
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
        let store = TomlSettingsStore::new(Some(path.clone()));

        let error = gtl_application::settings::remove_key::execute(
            gtl_application::settings::remove_key::RemoveSettingKey {
                key: "layout".into(),
            },
            &store,
        )
        .expect_err("non-string layout is rejected");

        assert!(matches!(
            error,
            gtl_application::settings::remove_key::RemoveSettingKeyError::InvalidValueShape { key }
                if key == "layout"
        ));
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }

    #[test]
    fn set_operation_adds_action_and_path_context_without_changing_malformed_toml() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let raw = b"theme = {{{\n";
        std::fs::write(&path, raw).expect("seed config");
        let store = TomlSettingsStore::new(Some(path.clone()));

        let error = gtl_application::settings::set_key::execute(
            gtl_application::settings::set_key::SetSettingKey {
                key: "theme".into(),
                value_new: "light".into(),
            },
            &store,
        )
        .expect_err("malformed TOML is rejected");
        let detail = match error {
            gtl_application::settings::set_key::SetSettingKeyError::Unexpected(error) => {
                format!("{error:#}")
            }
            error => panic!("expected unexpected error, got {error:?}"),
        };

        assert!(detail.contains("set user setting `theme`"), "{detail}");
        assert!(detail.contains(&path.display().to_string()), "{detail}");
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
