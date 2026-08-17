use nutype::nutype;

/// Counts persisted renders without eroding into a page ordinal or another count.
#[nutype(
    const_fn,
    default = 0,
    derive(
        Debug,
        Clone,
        Copy,
        Default,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct HistoryRenderCount(u64);

/// Identifies one page in recent-render history and mechanically excludes zero.
#[nutype(
    validate(greater_or_equal = 1),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct HistoryPageNumber(u32);

/// Counts pages in recent-render history and mechanically excludes zero.
#[nutype(
    validate(greater_or_equal = 1),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct HistoryPageCount(u32);

/// A valid page position in non-empty history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "HistoryPageData", into = "HistoryPageData")]
pub struct HistoryPage {
    number: HistoryPageNumber,
    count: HistoryPageCount,
}

impl HistoryPage {
    pub fn new(
        number: HistoryPageNumber,
        count: HistoryPageCount,
    ) -> Result<Self, InvalidHistoryPage> {
        if u32::from(number) > u32::from(count) {
            return Err(InvalidHistoryPage { number, count });
        }
        Ok(Self { number, count })
    }

    pub const fn number(self) -> HistoryPageNumber {
        self.number
    }

    pub const fn count(self) -> HistoryPageCount {
        self.count
    }

    pub fn previous(self) -> Option<HistoryPageNumber> {
        u32::from(self.number)
            .checked_sub(1)
            .and_then(|number| HistoryPageNumber::try_new(number).ok())
    }

    pub fn next(self) -> Option<HistoryPageNumber> {
        u32::from(self.number)
            .checked_add(1)
            .filter(|number| *number <= u32::from(self.count))
            .and_then(|number| HistoryPageNumber::try_new(number).ok())
    }

    pub fn progress_percent(self) -> u64 {
        let number = u64::from(u32::from(self.number));
        let count = u64::from(u32::from(self.count));
        number.saturating_mul(100) / count
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct HistoryPageData {
    number: HistoryPageNumber,
    count: HistoryPageCount,
}

impl TryFrom<HistoryPageData> for HistoryPage {
    type Error = InvalidHistoryPage;

    fn try_from(value: HistoryPageData) -> Result<Self, Self::Error> {
        Self::new(value.number, value.count)
    }
}

impl From<HistoryPage> for HistoryPageData {
    fn from(value: HistoryPage) -> Self {
        Self {
            number: value.number,
            count: value.count,
        }
    }
}

/// Distinguishes empty history from a validated non-empty page position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", content = "page", rename_all = "snake_case")]
pub enum HistoryPagePosition {
    Empty,
    Page(HistoryPage),
}

impl HistoryPagePosition {
    pub const fn page(self) -> Option<HistoryPage> {
        match self {
            Self::Empty => None,
            Self::Page(page) => Some(page),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("history page {number} exceeds page count {count}")]
pub struct InvalidHistoryPage {
    number: HistoryPageNumber,
    count: HistoryPageCount,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: u32) -> HistoryPageNumber {
        HistoryPageNumber::try_new(value).expect("fixture page number is positive")
    }

    fn count(value: u32) -> HistoryPageCount {
        HistoryPageCount::try_new(value).expect("fixture page count is positive")
    }

    #[test]
    fn zero_is_not_a_page_number_or_page_count() {
        assert!(HistoryPageNumber::try_new(0).is_err());
        assert!(HistoryPageCount::try_new(0).is_err());
    }

    #[test]
    fn a_page_cannot_exceed_the_history_page_count() {
        assert_eq!(
            HistoryPage::new(number(4), count(3)),
            Err(InvalidHistoryPage {
                number: number(4),
                count: count(3),
            })
        );
    }

    #[test]
    fn navigation_stays_inside_the_valid_page_range() {
        let first = HistoryPage::new(number(1), count(3)).expect("valid first page");
        let middle = HistoryPage::new(number(2), count(3)).expect("valid middle page");
        let last = HistoryPage::new(number(3), count(3)).expect("valid last page");

        assert_eq!(first.previous(), None);
        assert_eq!(first.next(), Some(number(2)));
        assert_eq!(middle.previous(), Some(number(1)));
        assert_eq!(middle.next(), Some(number(3)));
        assert_eq!(last.next(), None);
        assert_eq!(middle.progress_percent(), 66);
    }

    #[test]
    fn position_serde_preserves_the_closed_state() {
        let position = HistoryPagePosition::Page(
            HistoryPage::new(number(2), count(3)).expect("valid page position"),
        );
        let json = serde_json::to_value(position).expect("position serializes");

        assert_eq!(
            json,
            serde_json::json!({"state": "page", "page": {"number": 2, "count": 3}})
        );
        assert_eq!(
            serde_json::from_value::<HistoryPagePosition>(json).expect("position deserializes"),
            position
        );
        assert!(
            serde_json::from_value::<HistoryPagePosition>(
                serde_json::json!({"state": "page", "page": {"number": 4, "count": 3}}),
            )
            .is_err()
        );
    }
}
