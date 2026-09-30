use gtl_models::{diffs::CommitTimeRange, timestamps::MachineTimestamp};
use jiff::{Timestamp, tz::Offset};

use crate::views::diffs::changes_since::parse_local_input;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommitSearchTimeInput {
    pub from: String,
    pub until: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitSearchTimeError {
    From,
    Until,
    Reversed,
}

impl CommitSearchTimeInput {
    pub(crate) fn has_bounds(&self) -> bool {
        !self.from.is_empty() || !self.until.is_empty()
    }

    pub(crate) fn parse(
        &self,
        offset_at: impl Fn(Timestamp) -> Offset,
    ) -> Result<CommitTimeRange, CommitSearchTimeError> {
        CommitTimeRange::new(
            parse_bound(&self.from, &offset_at, CommitSearchTimeError::From)?,
            parse_bound(&self.until, &offset_at, CommitSearchTimeError::Until)?,
        )
        .map_err(|_| CommitSearchTimeError::Reversed)
    }
}

fn parse_bound(
    value: &str,
    offset_at: &impl Fn(Timestamp) -> Offset,
    error: CommitSearchTimeError,
) -> Result<Option<MachineTimestamp>, CommitSearchTimeError> {
    if value.is_empty() {
        return Ok(None);
    }
    let instant = parse_local_input(value, offset_at).ok_or(error)?;
    MachineTimestamp::try_from(instant.to_string())
        .map(Some)
        .map_err(|_| error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_times_produce_offset_qualified_bounds_and_allow_clearing() {
        let offset_at = |_| Offset::from_hours(-3).unwrap();
        let input = CommitSearchTimeInput {
            from: "2026-09-29T09:00:01".into(),
            until: "2026-09-30T10:00:02".into(),
        };
        let range = input.parse(offset_at).unwrap();
        assert_eq!(range.from().unwrap().as_ref(), "2026-09-29T12:00:01Z");
        assert_eq!(range.until().unwrap().as_ref(), "2026-09-30T13:00:02Z");
        assert!(
            CommitSearchTimeInput::default()
                .parse(offset_at)
                .unwrap()
                .contains(range.from().unwrap())
        );
        assert!(
            CommitSearchTimeInput {
                until: String::new(),
                ..input
            }
            .parse(offset_at)
            .unwrap()
            .until()
            .is_none()
        );
    }

    #[test]
    fn invalid_inputs_and_reversed_bounds_have_distinct_errors() {
        for (from, until, error) in [
            ("yesterday", "", CommitSearchTimeError::From),
            ("", "tomorrow", CommitSearchTimeError::Until),
            (
                "2026-09-30T12:00",
                "2026-09-29T12:00",
                CommitSearchTimeError::Reversed,
            ),
        ] {
            assert_eq!(
                CommitSearchTimeInput {
                    from: from.into(),
                    until: until.into()
                }
                .parse(|_| Offset::UTC),
                Err(error)
            );
        }
    }
}
