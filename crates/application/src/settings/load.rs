use std::collections::BTreeMap;

use domain::{
    diffs::DiffExclusions,
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};
use serde::Deserialize;

use crate::ports::AppSettings;

#[derive(Debug, Default, Deserialize)]
struct UserSettingsToml {
    theme: Option<toml::Value>,
    layout: Option<toml::Value>,
    density: Option<toml::Value>,
    #[serde(default)]
    push: PushSettingsToml,
    #[serde(default)]
    diff: DiffSettingsToml,
}

#[derive(Debug, Deserialize)]
struct PushSettingsToml {
    #[serde(default = "confirm_by_default")]
    confirm: bool,
}

#[derive(Debug, Default, Deserialize)]
struct DiffSettingsToml {
    #[serde(default)]
    exclude: BTreeMap<String, Vec<String>>,
}

impl Default for PushSettingsToml {
    fn default() -> Self {
        Self {
            confirm: confirm_by_default(),
        }
    }
}

const fn confirm_by_default() -> bool {
    true
}

fn parse_scalar<T: std::str::FromStr>(value: Option<toml::Value>) -> Option<T> {
    value?.as_str()?.parse().ok()
}

/// Parses raw user TOML into one safe effective settings snapshot.
pub fn from_toml(raw: &str) -> AppSettings {
    let config = toml::from_str::<UserSettingsToml>(raw).unwrap_or_default();
    let theme = parse_scalar::<Theme>(config.theme).map(|theme| theme.to_string());
    let layout = parse_scalar::<DiffLayout>(config.layout).unwrap_or(DiffLayout::Unified);
    let density = parse_scalar::<DiffDensity>(config.density).unwrap_or(DiffDensity::Compact);

    AppSettings::new(
        theme,
        config.push.confirm,
        DiffExclusions::new(config.diff.exclude, None),
    )
    .with_viewer_render_options(RenderOptions::new(layout, density))
}

#[cfg(test)]
mod tests {
    use domain::viewer::{DiffDensity, DiffLayout, RenderOptions};

    use super::from_toml;
    use crate::ports::AppSettings;

    #[test]
    fn checked_in_example_maps_every_effective_setting() {
        let settings = from_toml(include_str!("../../../../config/local/config.example.toml"));

        assert_eq!(settings.theme(), Some("dark"));
        assert_eq!(
            settings.viewer_render_options(),
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact)
        );
        assert!(settings.push_confirmation_required());
        let exclusions = settings.diff_exclusions();
        let exclusions_git_tools = exclusions.for_project_or_default("git-tools");
        let exclusions_default = exclusions.for_project_or_default("unconfigured");
        assert!(exclusions_git_tools.matches("frontend.js"));
        assert!(!exclusions_default.matches("frontend.js"));
        assert!(exclusions_default.matches("README.md"));
    }

    #[test]
    fn invalid_scalar_values_fall_back_independently() {
        let cases = [
            (
                "theme = 7\nlayout = \"split\"\ndensity = \"full\"\n[push]\nconfirm = false\n",
                None,
                RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                false,
            ),
            (
                "theme = \"hearth\"\nlayout = \"diagonal\"\ndensity = \"full\"\n",
                Some("hearth"),
                RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
                true,
            ),
            (
                "theme = \"light\"\nlayout = \"split\"\ndensity = false\n",
                Some("light"),
                RenderOptions::new(DiffLayout::Split, DiffDensity::Compact),
                true,
            ),
        ];

        for (
            raw,
            theme_expected,
            viewer_render_options_expected,
            push_confirmation_required_expected,
        ) in cases
        {
            let settings = from_toml(raw);

            assert_eq!(settings.theme(), theme_expected, "{raw}");
            assert_eq!(
                settings.viewer_render_options(),
                viewer_render_options_expected,
                "{raw}"
            );
            assert_eq!(
                settings.push_confirmation_required(),
                push_confirmation_required_expected,
                "{raw}"
            );
        }
    }

    #[test]
    fn malformed_toml_uses_the_complete_safe_default() {
        assert_eq!(from_toml("not valid toml {{{"), AppSettings::default());
    }
}
