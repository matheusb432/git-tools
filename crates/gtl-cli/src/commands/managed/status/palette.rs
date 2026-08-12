//! ANSI color palette for `status --color`, sourced from `config/status-colors.toml`.

use anyhow::{Context, anyhow};
use serde::Deserialize;

const STATUS_COLORS_TOML: &str = include_str!("../../../../../../config/status-colors.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StatusColorPalette {
    pub(super) brackets: Rgb,
    pub(super) ahead_arrow: Rgb,
    pub(super) checkmark: Rgb,
    pub(super) change_markers: Rgb,
}

impl StatusColorPalette {
    pub(super) fn from_embedded_toml() -> anyhow::Result<Self> {
        Self::from_toml(STATUS_COLORS_TOML)
    }

    fn from_toml(raw: &str) -> anyhow::Result<Self> {
        let file: StatusColorPaletteToml =
            toml::from_str(raw).context("failed to parse embedded status color palette TOML")?;
        Ok(Self {
            brackets: Rgb::from_hex(&file.brackets)
                .context("invalid brackets_color in status color palette")?,
            ahead_arrow: Rgb::from_hex(&file.ahead_arrow)
                .context("invalid ahead_arrow_color in status color palette")?,
            checkmark: Rgb::from_hex(&file.checkmark)
                .context("invalid checkmark_color in status color palette")?,
            change_markers: Rgb::from_hex(&file.change_markers)
                .context("invalid change_markers_color in status color palette")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct StatusColorPaletteToml {
    #[serde(rename = "brackets_color")]
    brackets: String,
    #[serde(rename = "ahead_arrow_color")]
    ahead_arrow: String,
    #[serde(rename = "checkmark_color")]
    checkmark: String,
    #[serde(rename = "change_markers_color")]
    change_markers: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    fn from_hex(value: &str) -> anyhow::Result<Self> {
        let hex = value
            .strip_prefix('#')
            .ok_or_else(|| anyhow!("hex color must start with '#': {value}"))?;
        if hex.len() != 6 {
            anyhow::bail!("hex color must use #rrggbb form: {value}");
        }

        Ok(Self {
            red: parse_hex_channel(value, &hex[0..2])?,
            green: parse_hex_channel(value, &hex[2..4])?,
            blue: parse_hex_channel(value, &hex[4..6])?,
        })
    }

    pub(super) fn paint(self, value: &str) -> String {
        format!(
            "\x1b[38;2;{};{};{}m{value}\x1b[39m",
            self.red, self.green, self.blue
        )
    }
}

fn parse_hex_channel(color: &str, channel: &str) -> anyhow::Result<u8> {
    u8::from_str_radix(channel, 16)
        .with_context(|| format!("hex color contains invalid digits: {color}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_colors_live_in_toml() {
        let raw = std::fs::read_to_string("../../config/status-colors.toml").unwrap();
        let file: StatusColorPaletteToml = toml::from_str(&raw).unwrap();

        assert_eq!(file.brackets, "#f28500");
        assert_eq!(file.ahead_arrow, "#f28500");
        assert_eq!(file.checkmark, "#2ecc71");
        assert_eq!(file.change_markers, "#ff4d4d");
    }
}
