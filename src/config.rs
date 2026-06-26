//! User TOML config for the diff renderer. A bad/missing config must never break
//! rendering, so every failure path degrades to [`GtlConfig::default`].

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Theme values the renderer knows how to honour; anything else resolves to `None`.
const KNOWN_THEMES: &[&str] = &["dark", "light", "hearth"];

/// Where a rendered diff is opened. `App` spawns the desktop viewer; `Browser`
/// opens from the store in the OS browser; `None` opens nothing. Default `App`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Viewer {
    #[default]
    App,
    Browser,
    None,
}

#[derive(Debug, Default, Deserialize)]
pub struct DiffConfig {
    #[serde(default)]
    pub viewer: Viewer,
}

#[derive(Debug, Default, Deserialize)]
pub struct GtlConfig {
    pub theme: Option<String>,
    #[serde(default)]
    pub diff: DiffConfig,
}

/// Parse + validate a config from raw TOML. Pure: a parse error or an unknown/missing
/// `theme` both collapse to a default (no panic, no error out).
pub fn from_toml(raw: &str) -> GtlConfig {
    let mut config = toml::from_str::<GtlConfig>(raw).unwrap_or_default();
    if !config
        .theme
        .as_deref()
        .is_some_and(|theme| KNOWN_THEMES.contains(&theme))
    {
        config.theme = None;
    }
    config
}

/// Read the config at `path` (if present) and parse it; any miss → default.
fn load_from(path: Option<PathBuf>) -> GtlConfig {
    let Some(path) = path else {
        return GtlConfig::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(raw) => from_toml(&raw),
        Err(_) => GtlConfig::default(),
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

/// Resolve the config path from the process environment (mirrors `home_dir_from_env`).
fn config_path() -> Option<PathBuf> {
    let env_override = std::env::var_os("GIT_TOOLS_CONFIG").map(PathBuf::from);
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    config_path_from(env_override, xdg_config_home, home)
}

/// Load the effective user config from the environment.
pub fn load() -> GtlConfig {
    load_from(config_path())
}

/// Set the `theme` key in `raw` TOML, preserving every other key, formatting, and
/// comment. Unlike the reader, a corrupt config is an error here — we refuse to
/// clobber a file we cannot parse.
fn set_theme_in_toml(raw: &str, theme: &str) -> Result<String> {
    let mut doc = raw
        .parse::<toml_edit::DocumentMut>()
        .context("parse existing config TOML")?;
    doc["theme"] = toml_edit::value(theme);
    Ok(doc.to_string())
}

/// Write `theme` into the config file at `path`, creating it (and any missing
/// parent directories) while preserving any existing content.
fn save_theme_to(path: &Path, theme: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let updated = set_theme_in_toml(&existing, theme)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }
    std::fs::write(path, updated).with_context(|| format!("write config {}", path.display()))?;
    Ok(())
}

/// Persist `theme` into the user config, resolving the path from the environment.
/// Returns the path written so the caller can report it.
pub fn save_theme(theme: &str) -> Result<PathBuf> {
    let path = config_path().context(
        "could not resolve a config path (no GIT_TOOLS_CONFIG, XDG_CONFIG_HOME, or HOME)",
    )?;
    save_theme_to(&path, theme)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn from_toml_accepts_known_theme() {
        assert_eq!(
            from_toml("theme = \"hearth\"").theme.as_deref(),
            Some("hearth")
        );
    }

    #[test]
    fn from_toml_empty_has_no_theme() {
        assert_eq!(from_toml("").theme, None);
    }

    #[test]
    fn from_toml_rejects_unknown_theme() {
        assert_eq!(from_toml("theme = \"bogus\"").theme, None);
    }

    #[test]
    fn from_toml_invalid_toml_does_not_panic() {
        assert_eq!(from_toml("not valid toml {{{").theme, None);
    }

    #[test]
    fn load_from_reads_theme_from_file() {
        let mut file = NamedTempFile::new().expect("create temp config");
        write!(file, "theme = \"light\"").expect("write temp config");
        assert_eq!(
            load_from(Some(file.path().to_path_buf())).theme.as_deref(),
            Some("light")
        );
    }

    #[test]
    fn load_from_none_is_default() {
        assert_eq!(load_from(None).theme, None);
    }

    #[test]
    fn load_from_nonexistent_path_is_default() {
        assert_eq!(
            load_from(Some(PathBuf::from("/no/such/git-tools/config.toml"))).theme,
            None
        );
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

    #[test]
    fn from_toml_reads_viewer_app_and_browser() {
        assert_eq!(
            from_toml("[diff]\nviewer = \"app\"").diff.viewer,
            Viewer::App
        );
        assert_eq!(
            from_toml("[diff]\nviewer = \"browser\"").diff.viewer,
            Viewer::Browser
        );
    }

    #[test]
    fn from_toml_defaults_viewer_to_app() {
        // ! P2 flips the default from browser to app (the launch path now exists).
        assert_eq!(from_toml("").diff.viewer, Viewer::App);
    }

    #[test]
    fn from_toml_unknown_viewer_falls_back_to_default_app() {
        assert_eq!(
            from_toml("[diff]\nviewer = \"bogus\"").diff.viewer,
            Viewer::App
        );
    }

    #[test]
    fn from_toml_flat_viewer_is_ignored() {
        // A flat top-level `viewer` key must NOT be honoured — the schema is nested.
        // Users who set it without a `[diff]` header stay on the default (App).
        assert_eq!(from_toml("viewer = \"browser\"").diff.viewer, Viewer::App);
    }

    #[test]
    fn set_theme_in_toml_writes_into_empty_config() {
        assert_eq!(set_theme_in_toml("", "dark").unwrap(), "theme = \"dark\"\n");
    }

    #[test]
    fn set_theme_in_toml_overwrites_existing_theme() {
        let updated = set_theme_in_toml("theme = \"light\"\n", "hearth").unwrap();
        assert_eq!(updated, "theme = \"hearth\"\n");
    }

    #[test]
    fn set_theme_in_toml_preserves_other_keys_and_comments() {
        let raw = "# my config\ntheme = \"light\"\n\n[diff]\nviewer = \"browser\" # keep me\n";
        let updated = set_theme_in_toml(raw, "dark").unwrap();
        assert!(
            updated.contains("# my config"),
            "comment dropped: {updated}"
        );
        assert!(
            updated.contains("theme = \"dark\""),
            "theme not set: {updated}"
        );
        assert!(
            updated.contains("viewer = \"browser\" # keep me"),
            "viewer/comment dropped: {updated}"
        );
    }

    #[test]
    fn set_theme_in_toml_rejects_unparseable_config() {
        assert!(set_theme_in_toml("not valid toml {{{", "dark").is_err());
    }

    #[test]
    fn save_theme_to_creates_file_that_round_trips_through_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("git-tools").join("config.toml");
        save_theme_to(&path, "hearth").expect("save theme");
        assert_eq!(load_from(Some(path)).theme.as_deref(), Some("hearth"));
    }

    #[test]
    fn save_theme_to_preserves_an_existing_viewer_setting() {
        let mut file = NamedTempFile::new().expect("create temp config");
        write!(file, "[diff]\nviewer = \"browser\"\n").expect("seed config");
        save_theme_to(file.path(), "light").expect("save theme");
        let config = load_from(Some(file.path().to_path_buf()));
        assert_eq!(config.theme.as_deref(), Some("light"));
        assert_eq!(config.diff.viewer, Viewer::Browser);
    }
}
