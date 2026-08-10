use std::str::FromStr;

use thiserror::Error;

use crate::viewer::{DiffDensity, DiffLayout, ParseRenderOptionError, Theme};

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SettingKeyValue {
    Theme(Theme),
    Layout(DiffLayout),
    Density(DiffDensity),
}

// TODO: refactor every inlined string usage of these into the constants usage.
pub const THEME: &str = "theme";
pub const LAYOUT: &str = "layout";
pub const DENSITY: &str = "density";

impl SettingKeyValue {
    pub fn new(key: &str, value: &str) -> Result<Self, SettingKeyValueError> {
        Ok(match key {
            // TODO: briefer way to write this??
            THEME => SettingKeyValue::Theme(Theme::from_str(value)?),
            LAYOUT => SettingKeyValue::Layout(DiffLayout::from_str(value)?),
            DENSITY => SettingKeyValue::Density(DiffDensity::from_str(value)?),
            _ => {
                return Err(SettingKeyValueError::InvalidKey {
                    key: key.to_owned(),
                });
            }
        })
    }

    // TODO: create newtype for the key itself, this should not return a string.
    pub fn key(&self) -> String {
        match self {
            SettingKeyValue::Theme(_) => THEME,
            SettingKeyValue::Layout(_) => LAYOUT,
            SettingKeyValue::Density(_) => DENSITY,
        }
        .to_string()
    }
}

#[derive(Debug, Error)]
pub enum SettingKeyValueError {
    #[error("the key `{key}` is not a supported user setting")]
    InvalidKey { key: String },
    #[error(transparent)]
    ValueParseError(#[from] ParseRenderOptionError),
}
