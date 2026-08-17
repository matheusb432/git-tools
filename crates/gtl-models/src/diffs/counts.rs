use std::fmt;

/// Counts added or removed source lines without eroding into unrelated integers.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct DiffLineCount(u64);

impl DiffLineCount {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric count for final presentation or algorithm boundaries.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Increments a parser-owned count, saturating only at the representable maximum.
    pub fn increment(&mut self) {
        *self = Self::new(self.0.saturating_add(1));
    }

    /// Adds another count for presentation totals.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::new(self.0.saturating_add(other.0))
    }
}

impl fmt::Display for DiffLineCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::DiffLineCount;

    #[test]
    fn serde_preserves_the_numeric_wire_shape() {
        let count = DiffLineCount::new(42);

        assert_eq!(serde_json::to_string(&count).unwrap(), "42");
        assert_eq!(serde_json::from_str::<DiffLineCount>("42").unwrap(), count);
    }

    #[test]
    fn counts_accumulate_without_crossing_into_other_units() {
        let total = DiffLineCount::new(2).saturating_add(DiffLineCount::new(3));

        assert_eq!(total.value(), 5);
    }
}
