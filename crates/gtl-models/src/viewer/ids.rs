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
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFrom, Into, Display)
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
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFrom, Into, Display)
)]
pub struct RenderHistoryId(i64);

#[cfg(test)]
mod tests {
    use super::super::{RenderHistoryId, ViewerTabId};

    #[test]
    fn zero_is_not_a_viewer_tab_id() {
        assert!(ViewerTabId::try_new(0).is_err());
        assert_eq!(u64::from(ViewerTabId::try_new(7).expect("positive id")), 7);
    }

    #[test]
    fn non_positive_values_are_not_render_history_ids() {
        assert!(RenderHistoryId::try_new(-1).is_err());
        assert!(RenderHistoryId::try_new(0).is_err());
        assert_eq!(
            i64::from(RenderHistoryId::try_new(11).expect("positive id")),
            11
        );
    }
}
