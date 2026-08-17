use std::{fmt, num::NonZeroU32};

use nutype::nutype;

/// A line number in the old or new source represented by a diff.
#[nutype(
    const_fn,
    default = 0,
    derive(
        Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Display
    )
)]
pub struct SourceLineNumber(u32);

/// A count of Unicode scalar values in source text.
#[nutype(
    const_fn,
    derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Display)
)]
pub struct CharacterCount(usize);

/// A character offset measured in Unicode scalar values from the start of source text.
#[nutype(
    const_fn,
    constructor(visibility = pub(crate)),
    derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)
)]
pub struct CharacterOffset(usize);

/// A decimal digit count used to size source-line-number gutters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineNumberDigitWidth(NonZeroU32);

impl LineNumberDigitWidth {
    const MIN: Self = Self(NonZeroU32::MIN);

    pub(crate) const fn from_source_line_number_max(line_number_max: SourceLineNumber) -> Self {
        let digits = match line_number_max.into_inner().checked_ilog10() {
            Some(digits) => digits + 1,
            None => Self::MIN.0.get(),
        };
        match NonZeroU32::new(digits) {
            Some(digits) => Self(digits),
            None => Self::MIN,
        }
    }

    /// Returns the number of decimal digits.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LineNumberDigitWidth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Default for LineNumberDigitWidth {
    fn default() -> Self {
        Self::MIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_roles_preserve_numeric_values() {
        assert_eq!(SourceLineNumber::new(42).into_inner(), 42);
        assert_eq!(CharacterCount::new(2_001).into_inner(), 2_001);
        assert_eq!(CharacterOffset::new(17).into_inner(), 17);
    }

    #[test]
    fn line_number_digit_width_covers_u32_decimal_bounds() {
        assert_eq!(
            LineNumberDigitWidth::from_source_line_number_max(SourceLineNumber::default()).get(),
            1
        );
        assert_eq!(
            LineNumberDigitWidth::from_source_line_number_max(SourceLineNumber::new(9)).get(),
            1
        );
        assert_eq!(
            LineNumberDigitWidth::from_source_line_number_max(SourceLineNumber::new(10)).get(),
            2
        );
        assert_eq!(
            LineNumberDigitWidth::from_source_line_number_max(SourceLineNumber::new(u32::MAX))
                .get(),
            10
        );
    }
}
