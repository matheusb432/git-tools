use serde::{Deserialize, Serialize};

use crate::timestamps::MachineTimestamp;

/// Inclusive commit timestamp bounds; either bound may be absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CommitTimeRangeInput")]
pub struct CommitTimeRange {
    from: Option<MachineTimestamp>,
    until: Option<MachineTimestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("commit time range starts after it ends")]
pub struct CommitTimeRangeError;

impl CommitTimeRange {
    pub fn new(
        from: Option<MachineTimestamp>,
        until: Option<MachineTimestamp>,
    ) -> Result<Self, CommitTimeRangeError> {
        if from
            .as_ref()
            .zip(until.as_ref())
            .is_some_and(|(from, until)| from > until)
        {
            return Err(CommitTimeRangeError);
        }
        Ok(Self { from, until })
    }

    #[must_use]
    pub fn from(&self) -> Option<&MachineTimestamp> {
        self.from.as_ref()
    }

    #[must_use]
    pub fn until(&self) -> Option<&MachineTimestamp> {
        self.until.as_ref()
    }

    #[must_use]
    pub fn contains(&self, timestamp: &MachineTimestamp) -> bool {
        self.from.as_ref().is_none_or(|from| timestamp >= from)
            && self.until.as_ref().is_none_or(|until| timestamp <= until)
    }
}

#[derive(Deserialize)]
struct CommitTimeRangeInput {
    from: Option<MachineTimestamp>,
    until: Option<MachineTimestamp>,
}

impl TryFrom<CommitTimeRangeInput> for CommitTimeRange {
    type Error = CommitTimeRangeError;

    fn try_from(input: CommitTimeRangeInput) -> Result<Self, Self::Error> {
        Self::new(input.from, input.until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> MachineTimestamp {
        value.try_into().unwrap()
    }

    #[test]
    fn inclusive_bounds_compare_instants_and_allow_open_ranges() {
        let from = timestamp("2026-09-29T09:00:00-03:00");
        let until = timestamp("2026-09-30T12:00:00Z");
        let before = timestamp("2026-09-29T11:59:59Z");
        let after = timestamp("2026-09-30T12:00:01Z");
        let range = CommitTimeRange::new(Some(from.clone()), Some(until.clone())).unwrap();
        assert!(!range.contains(&before));
        assert!(range.contains(&timestamp("2026-09-29T12:00:00Z")));
        assert!(range.contains(&until));
        assert!(!range.contains(&after));
        assert!(
            CommitTimeRange::new(Some(from), None)
                .unwrap()
                .contains(&after)
        );
        assert!(
            CommitTimeRange::new(None, Some(until))
                .unwrap()
                .contains(&before)
        );
        assert!(CommitTimeRange::default().contains(&after));
    }

    #[test]
    fn construction_and_deserialization_reject_reversed_bounds() {
        let from = timestamp("2026-09-30T12:00:00Z");
        let until = timestamp("2026-09-29T12:00:00Z");
        assert_eq!(
            CommitTimeRange::new(Some(from.clone()), Some(until)),
            Err(CommitTimeRangeError)
        );
        assert!(CommitTimeRange::new(Some(from.clone()), Some(from)).is_ok());
        assert!(
            serde_json::from_str::<CommitTimeRange>(
                r#"{"from":"2026-09-30T12:00:00Z","until":"2026-09-29T12:00:00Z"}"#,
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CommitTimeRange>(r#"{"from":"2026-09-30T12:00:00"}"#).is_err()
        );
    }
}
