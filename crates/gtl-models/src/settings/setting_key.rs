use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::viewer::{DiffDensity, DiffLayout, ParseRenderOptionError, Theme};

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SettingKeyValue {
    Theme(Theme),
    Layout(DiffLayout),
    Density(DiffDensity),
}

/// Identifies one supported scalar user setting.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SettingKey {
    Theme,
    Layout,
    Density,
}

impl SettingKey {
    /// Returns the stable root key used in the TOML document.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::Density => "density",
        }
    }
}

impl fmt::Display for SettingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SettingKey {
    type Err = ParseSettingKeyError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "theme" => Ok(Self::Theme),
            "layout" => Ok(Self::Layout),
            "density" => Ok(Self::Density),
            _ => Err(ParseSettingKeyError {
                key: raw.to_owned(),
            }),
        }
    }
}

impl SettingKeyValue {
    /// Parses a raw setting pair at an input boundary.
    pub fn parse(key: &str, value: &str) -> Result<Self, SettingKeyValueError> {
        Self::new(key.parse()?, value)
    }

    /// Parses a raw value for a validated setting key.
    pub fn new(key: SettingKey, value: &str) -> Result<Self, SettingKeyValueError> {
        Ok(match key {
            SettingKey::Theme => Self::Theme(Theme::from_str(value)?),
            SettingKey::Layout => Self::Layout(DiffLayout::from_str(value)?),
            SettingKey::Density => Self::Density(DiffDensity::from_str(value)?),
        })
    }

    /// Returns the key selected by this typed mutation.
    #[must_use]
    pub const fn key(self) -> SettingKey {
        match self {
            Self::Theme(_) => SettingKey::Theme,
            Self::Layout(_) => SettingKey::Layout,
            Self::Density(_) => SettingKey::Density,
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingKeyValueError {
    #[error(transparent)]
    Key(#[from] ParseSettingKeyError),
    #[error(transparent)]
    ValueParseError(#[from] ParseRenderOptionError),
}

/// Reports an unsupported root user-setting key.
#[derive(Debug, Error, Eq, PartialEq)]
#[error("the key `{key}` is not a supported user setting")]
pub struct ParseSettingKeyError {
    pub key: String,
}

#[cfg(test)]
mod tests {
    use super::{SettingKey, SettingKeyValue};
    use crate::viewer::{DiffDensity, DiffLayout, Theme};

    #[test]
    fn raw_pairs_cross_into_typed_mutations_once() {
        assert_eq!(
            SettingKeyValue::parse("theme", "mirage").unwrap(),
            SettingKeyValue::Theme(Theme::Mirage)
        );
        assert_eq!(
            SettingKeyValue::parse("layout", "split").unwrap(),
            SettingKeyValue::Layout(DiffLayout::Split)
        );
        assert_eq!(
            SettingKeyValue::parse("density", "full").unwrap(),
            SettingKeyValue::Density(DiffDensity::Full)
        );
    }

    #[test]
    fn mutation_exposes_its_matching_key() {
        let mutation = SettingKeyValue::Theme(Theme::Glacier);

        assert_eq!(mutation.key(), SettingKey::Theme);
    }
}
