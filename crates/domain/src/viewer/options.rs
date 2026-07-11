use std::{fmt, str::FromStr};

/// Selects the structural arrangement used to render diff rows.
///
/// # Examples
///
/// ```
/// use domain::viewer::DiffLayout;
///
/// assert_eq!("split".parse(), Ok(DiffLayout::Split));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffLayout {
    /// Renders old and new lines in one column.
    Unified,
    /// Renders old and new lines side by side.
    Split,
}

impl FromStr for DiffLayout {
    type Err = ParseRenderOptionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "unified" => Ok(Self::Unified),
            "split" => Ok(Self::Split),
            value => Err(ParseRenderOptionError::Layout {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for DiffLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unified => "unified",
            Self::Split => "split",
        })
    }
}

/// Selects how much supporting detail accompanies rendered diff rows.
///
/// # Examples
///
/// ```
/// use domain::viewer::DiffDensity;
///
/// assert_eq!("full".parse(), Ok(DiffDensity::Full));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffDensity {
    /// Renders the condensed diff presentation.
    Compact,
    /// Renders the complete diff presentation.
    Full,
}

impl FromStr for DiffDensity {
    type Err = ParseRenderOptionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "compact" => Ok(Self::Compact),
            "full" => Ok(Self::Full),
            value => Err(ParseRenderOptionError::Density {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for DiffDensity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Compact => "compact",
            Self::Full => "full",
        })
    }
}

/// Selects the viewer's visual color theme.
///
/// # Examples
///
/// ```
/// use domain::viewer::Theme;
///
/// assert_eq!("hearth".parse(), Ok(Theme::Hearth));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Uses the dark neutral palette.
    Dark,
    /// Uses the light neutral palette.
    Light,
    /// Uses the warm hearth palette.
    Hearth,
}

impl FromStr for Theme {
    type Err = ParseRenderOptionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "hearth" => Ok(Self::Hearth),
            value => Err(ParseRenderOptionError::Theme {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Hearth => "hearth",
        })
    }
}

/// Describes a rejected closed-set viewer rendering option.
///
/// # Examples
///
/// ```
/// use domain::viewer::{DiffLayout, ParseRenderOptionError};
///
/// let error = "wide".parse::<DiffLayout>().expect_err("wide is not a layout");
/// assert!(matches!(error, ParseRenderOptionError::Layout { value } if value == "wide"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseRenderOptionError {
    /// Reports an unknown diff layout token.
    #[error("unknown diff layout `{value}`; expected `unified` or `split`")]
    Layout { value: String },
    /// Reports an unknown diff density token.
    #[error("unknown diff density `{value}`; expected `compact` or `full`")]
    Density { value: String },
    /// Reports an unknown viewer theme token.
    #[error("unknown viewer theme `{value}`; expected `dark`, `light`, or `hearth`")]
    Theme { value: String },
}

/// Holds the closed layout and density choices used for one diff rendering.
///
/// # Examples
///
/// ```
/// use domain::viewer::{DiffDensity, DiffLayout, RenderOptions};
///
/// let options = RenderOptions::try_from(("split", "full")).expect("known options");
/// assert_eq!(options.layout(), DiffLayout::Split);
/// assert_eq!(options.density(), DiffDensity::Full);
/// let error = RenderOptions::try_from(("wide", "full")).expect_err("unknown layout");
/// assert!(matches!(error, domain::viewer::ParseRenderOptionError::Layout { value } if value == "wide"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderOptions {
    layout: DiffLayout,
    density: DiffDensity,
}

impl RenderOptions {
    /// Provides the default unified, compact rendering options.
    pub const DEFAULT: Self = Self::new(DiffLayout::Unified, DiffDensity::Compact);

    /// Creates rendering options from validated closed-set values.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{DiffDensity, DiffLayout, RenderOptions};
    ///
    /// let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
    /// assert_eq!(options.layout(), DiffLayout::Split);
    /// ```
    pub const fn new(layout: DiffLayout, density: DiffDensity) -> Self {
        Self { layout, density }
    }

    /// Returns the selected diff layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{DiffLayout, RenderOptions};
    ///
    /// assert_eq!(RenderOptions::DEFAULT.layout(), DiffLayout::Unified);
    /// ```
    pub const fn layout(self) -> DiffLayout {
        self.layout
    }

    /// Returns the selected diff density.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{DiffDensity, RenderOptions};
    ///
    /// assert_eq!(RenderOptions::DEFAULT.density(), DiffDensity::Compact);
    /// ```
    pub const fn density(self) -> DiffDensity {
        self.density
    }

    /// Returns a copy with a different validated layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{DiffLayout, RenderOptions};
    ///
    /// assert_eq!(
    ///     RenderOptions::DEFAULT
    ///         .with_layout(DiffLayout::Split)
    ///         .layout(),
    ///     DiffLayout::Split
    /// );
    /// ```
    #[must_use]
    pub const fn with_layout(self, layout: DiffLayout) -> Self {
        Self::new(layout, self.density)
    }

    /// Returns a copy with a different validated density.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::viewer::{DiffDensity, RenderOptions};
    ///
    /// assert_eq!(
    ///     RenderOptions::DEFAULT
    ///         .with_density(DiffDensity::Full)
    ///         .density(),
    ///     DiffDensity::Full
    /// );
    /// ```
    #[must_use]
    pub const fn with_density(self, density: DiffDensity) -> Self {
        Self::new(self.layout, density)
    }
}

/// Parses closed layout and density tokens into rendering options.
///
/// # Examples
///
/// ```
/// use domain::viewer::{DiffLayout, ParseRenderOptionError, RenderOptions};
///
/// let options = RenderOptions::try_from(("split", "full")).expect("known options");
/// assert_eq!(options.layout(), DiffLayout::Split);
/// let error = RenderOptions::try_from(("wide", "full")).expect_err("unknown layout");
/// assert!(matches!(error, ParseRenderOptionError::Layout { value } if value == "wide"));
/// ```
impl TryFrom<(&str, &str)> for RenderOptions {
    type Error = ParseRenderOptionError;

    fn try_from((layout, density): (&str, &str)) -> Result<Self, Self::Error> {
        Ok(Self::new(layout.parse()?, density.parse()?))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DiffDensity, DiffLayout, RenderOptions, Theme};

    #[test]
    fn layout_parsing_returns_the_rejected_value() {
        let error = "wide"
            .parse::<DiffLayout>()
            .expect_err("unknown layout rejects");

        assert_eq!(
            error.to_string(),
            "unknown diff layout `wide`; expected `unified` or `split`"
        );
    }

    #[test]
    fn render_options_are_built_only_from_valid_values() {
        let options = RenderOptions::try_from(("split", "full")).expect("known tokens parse");

        assert_eq!(options.layout(), DiffLayout::Split);
        assert_eq!(options.density(), DiffDensity::Full);
    }

    #[test]
    fn option_tokens_round_trip_through_display() {
        assert_eq!(DiffLayout::Unified.to_string(), "unified");
        assert_eq!(DiffDensity::Compact.to_string(), "compact");
        assert_eq!(Theme::Hearth.to_string(), "hearth");
    }
}
