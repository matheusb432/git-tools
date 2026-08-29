use nutype::nutype;

/// Identifies one viewer tab and mechanically excludes zero.
///
/// # Examples
///
/// ```
/// use gtl_models::viewer::ViewerTabId;
///
/// let id = ViewerTabId::try_new(7).expect("positive ids are valid");
/// assert_eq!(u64::from(id), 7);
/// let error = ViewerTabId::try_new(0).expect_err("zero is invalid");
/// fn assert_typed_error(error: &impl std::error::Error) {}
/// assert_typed_error(&error);
/// ```
#[nutype(
    validate(greater_or_equal = 1),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct ViewerTabId(u64);

/// Identifies one persisted recent render and mechanically excludes non-positive row IDs.
///
/// # Examples
///
/// ```
/// use gtl_models::viewer::RenderHistoryId;
///
/// let id = RenderHistoryId::try_new(11).expect("positive ids are valid");
/// assert_eq!(i64::from(id), 11);
/// let error = RenderHistoryId::try_new(0).expect_err("zero is invalid");
/// fn assert_typed_error(error: &impl std::error::Error) {}
/// assert_typed_error(&error);
/// ```
#[nutype(
    validate(greater_or_equal = 1),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct RenderHistoryId(i64);

/// Orders full-range computations for one viewer tab.
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
pub struct ViewerRangeGeneration(u64);

impl ViewerRangeGeneration {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self::new(self.0.wrapping_add(1))
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Orders commit-selection computations independently from full-range computations.
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
pub struct ViewerSelectionGeneration(u64);

impl ViewerSelectionGeneration {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self::new(self.0.wrapping_add(1))
    }

    #[must_use]
    pub const fn previous(self) -> Self {
        Self::new(self.0.wrapping_sub(1))
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Orders authoritative viewer-shell snapshots and change notifications.
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
pub struct ViewerVersion(u64);

impl ViewerVersion {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    /// Returns the next process-local shell version.
    ///
    /// # Panics
    ///
    /// Panics after version `u64::MAX`; the server never wraps a viewer version.
    pub const fn next(self) -> Self {
        assert!(self.0 < u64::MAX, "viewer version exhausted u64");
        Self::new(self.0 + 1)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::super::{RenderHistoryId, ViewerTabId, ViewerVersion};

    #[test]
    fn zero_is_not_a_viewer_tab_id() {
        assert!(ViewerTabId::try_new(0).is_err());
        assert_eq!(u64::from(ViewerTabId::try_new(7).unwrap()), 7);
    }

    #[test]
    fn non_positive_values_are_not_render_history_ids() {
        assert!(RenderHistoryId::try_new(-1).is_err());
        assert!(RenderHistoryId::try_new(0).is_err());
        assert_eq!(i64::from(RenderHistoryId::try_new(11).unwrap()), 11);
    }

    #[test]
    #[should_panic(expected = "viewer version exhausted u64")]
    fn viewer_version_does_not_wrap() {
        let _ = ViewerVersion::new(u64::MAX).next();
    }
}
