use gtl_models::timestamps::MachineTimestamp;
use jiff::{SignedDuration, Timestamp, civil::DateTime, tz::Offset};

/// A quick cutoff for the changes-since filter, resolved once when chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangesSincePreset {
    LastHour,
    Today,
    Last24Hours,
    Last7Days,
}

impl ChangesSincePreset {
    pub(crate) const ALL: [Self; 4] = [
        Self::LastHour,
        Self::Today,
        Self::Last24Hours,
        Self::Last7Days,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LastHour => "last-hour",
            Self::Today => "today",
            Self::Last24Hours => "last-24-hours",
            Self::Last7Days => "last-7-days",
        }
    }

    pub(crate) fn from_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.as_str() == value)
    }

    /// Returns the cutoff for `now`, with "today" starting at local midnight.
    pub(crate) fn resolve(self, now: Timestamp, offset: Offset) -> Timestamp {
        let hours = |count: i64| now - SignedDuration::from_hours(count);
        match self {
            Self::LastHour => hours(1),
            Self::Last24Hours => hours(24),
            Self::Last7Days => hours(24 * 7),
            Self::Today => {
                let midnight = offset
                    .to_datetime(now)
                    .date()
                    .to_datetime(jiff::civil::Time::midnight());
                offset.to_timestamp(midnight).unwrap_or(now)
            }
        }
    }
}

/// The last changes-since choice made in one tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ChangesSinceChoice {
    #[default]
    AnyTime,
    Preset(ChangesSincePreset, MachineTimestamp),
    Custom,
}

/// The option the changes-since control shows for the applied cutoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangesSinceSelection {
    AnyTime,
    Preset(ChangesSincePreset),
    Custom,
}

impl ChangesSinceSelection {
    pub(crate) fn new(applied: Option<&MachineTimestamp>, choice: &ChangesSinceChoice) -> Self {
        match (applied, choice) {
            (None, ChangesSinceChoice::Custom) => Self::Custom,
            (None, _) => Self::AnyTime,
            (Some(applied), ChangesSinceChoice::Preset(preset, resolved))
                if applied == resolved =>
            {
                Self::Preset(*preset)
            }
            (Some(_), _) => Self::Custom,
        }
    }

    pub(crate) const fn value(self) -> &'static str {
        match self {
            Self::AnyTime => "any-time",
            Self::Preset(preset) => preset.as_str(),
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangesSinceEdit {
    AnyTime,
    Preset(ChangesSincePreset),
    Custom,
    CustomValue(String),
}

impl ChangesSinceEdit {
    pub(crate) fn from_selection_value(value: &str) -> Self {
        if value == ChangesSinceSelection::Custom.value() {
            return Self::Custom;
        }
        ChangesSincePreset::from_value(value).map_or(Self::AnyTime, Self::Preset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CutoffChange {
    Keep,
    Clear,
    Apply(MachineTimestamp),
}

impl ChangesSinceEdit {
    /// Returns the choice to remember and the cutoff change, or `None` for an unreadable time.
    pub(crate) fn resolve(
        self,
        now: Timestamp,
        offset_at: impl Fn(Timestamp) -> Offset,
    ) -> Option<(ChangesSinceChoice, CutoffChange)> {
        Some(match self {
            Self::AnyTime => (ChangesSinceChoice::AnyTime, CutoffChange::Clear),
            Self::Preset(preset) => {
                let cutoff = machine_timestamp(preset.resolve(now, offset_at(now)))?;
                (
                    ChangesSinceChoice::Preset(preset, cutoff.clone()),
                    CutoffChange::Apply(cutoff),
                )
            }
            Self::Custom => (ChangesSinceChoice::Custom, CutoffChange::Keep),
            Self::CustomValue(value) => {
                let cutoff = machine_timestamp(parse_local_input(&value, offset_at)?)?;
                (ChangesSinceChoice::Custom, CutoffChange::Apply(cutoff))
            }
        })
    }
}

pub(crate) fn machine_timestamp(instant: Timestamp) -> Option<MachineTimestamp> {
    MachineTimestamp::from_unix_seconds(instant.as_second()).ok()
}

/// Formats `timestamp` for a `datetime-local` input in the given local offset.
pub(crate) fn local_input_value(timestamp: &MachineTimestamp, offset: Offset) -> String {
    let local = offset.to_datetime(timestamp.instant());
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute()
    )
}

/// Reads a `datetime-local` value in the local offset that applies at that time.
pub(crate) fn parse_local_input(
    value: &str,
    offset_at: impl Fn(Timestamp) -> Offset,
) -> Option<Timestamp> {
    let local = value.parse::<DateTime>().ok()?;
    let mut instant = Offset::UTC.to_timestamp(local).ok()?;
    for _ in 0..3 {
        let candidate = offset_at(instant).to_timestamp(local).ok()?;
        if offset_at(candidate).to_datetime(candidate) == local {
            return Some(candidate);
        }
        instant = candidate;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(value: &str) -> Timestamp {
        value.parse().unwrap()
    }

    #[test]
    fn presets_resolve_against_the_current_time_and_local_midnight() {
        let now = instant("2026-09-28T14:30:00Z");
        let offset = Offset::from_hours(-3).unwrap();

        assert_eq!(
            ChangesSincePreset::LastHour.resolve(now, offset),
            instant("2026-09-28T13:30:00Z")
        );
        assert_eq!(
            ChangesSincePreset::Last7Days.resolve(now, offset),
            instant("2026-09-21T14:30:00Z")
        );
        assert_eq!(
            ChangesSincePreset::Today.resolve(now, offset),
            instant("2026-09-28T03:00:00Z")
        );
    }

    #[test]
    fn a_preset_shows_only_while_its_resolved_cutoff_is_applied() {
        let resolved = machine_timestamp(instant("2026-09-28T13:30:00Z")).unwrap();
        let other = machine_timestamp(instant("2026-09-27T13:30:00Z")).unwrap();
        let choice = ChangesSinceChoice::Preset(ChangesSincePreset::LastHour, resolved.clone());

        assert_eq!(
            ChangesSinceSelection::new(Some(&resolved), &choice),
            ChangesSinceSelection::Preset(ChangesSincePreset::LastHour)
        );
        assert_eq!(
            ChangesSinceSelection::new(Some(&other), &choice),
            ChangesSinceSelection::Custom
        );
        assert_eq!(
            ChangesSinceSelection::new(None, &choice),
            ChangesSinceSelection::AnyTime
        );
        assert_eq!(
            ChangesSinceSelection::new(None, &ChangesSinceChoice::Custom),
            ChangesSinceSelection::Custom
        );
    }

    #[test]
    fn edits_resolve_to_a_remembered_choice_and_a_cutoff_change() {
        let now = instant("2026-09-28T14:30:00Z");
        let offset = |_| Offset::UTC;
        let hour_ago = machine_timestamp(instant("2026-09-28T13:30:00Z")).unwrap();

        assert_eq!(
            ChangesSinceEdit::Preset(ChangesSincePreset::LastHour).resolve(now, offset),
            Some((
                ChangesSinceChoice::Preset(ChangesSincePreset::LastHour, hour_ago.clone()),
                CutoffChange::Apply(hour_ago),
            ))
        );
        assert_eq!(
            ChangesSinceEdit::Custom.resolve(now, offset),
            Some((ChangesSinceChoice::Custom, CutoffChange::Keep))
        );
        assert_eq!(
            ChangesSinceEdit::AnyTime.resolve(now, offset),
            Some((ChangesSinceChoice::AnyTime, CutoffChange::Clear))
        );
        assert_eq!(
            ChangesSinceEdit::CustomValue("soon".to_owned()).resolve(now, offset),
            None
        );
    }

    #[test]
    fn local_input_values_round_trip_in_the_local_offset() {
        let offset = Offset::from_hours(-3).unwrap();
        let timestamp = machine_timestamp(instant("2026-09-28T20:00:00Z")).unwrap();

        let value = local_input_value(&timestamp, offset);

        assert_eq!(value, "2026-09-28T17:00");
        assert_eq!(
            parse_local_input(&value, |_| offset),
            Some(timestamp.instant())
        );
        assert_eq!(parse_local_input("yesterday", |_| offset), None);
    }

    #[test]
    fn local_input_uses_the_offset_at_the_resolved_instant_and_rejects_dst_gaps() {
        let transition = instant("2026-03-08T07:00:00Z");
        let offset_at = |time| Offset::from_hours(if time < transition { -5 } else { -4 }).unwrap();
        assert_eq!(
            parse_local_input("2026-03-08T03:30", offset_at),
            Some(instant("2026-03-08T07:30:00Z"))
        );
        assert_eq!(
            parse_local_input("2026-03-08T01:30", offset_at),
            Some(instant("2026-03-08T06:30:00Z"))
        );
        assert_eq!(parse_local_input("2026-03-08T02:30", offset_at), None);
    }
}
