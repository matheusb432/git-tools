//! Validated machine timestamps with stable external representations.

use std::{cmp::Ordering, fmt, str::FromStr};

use jiff::{Timestamp, civil::DateTime, tz::Offset};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

#[derive(Debug, thiserror::Error)]
pub enum TimestampError {
    #[error("timestamp is not an offset-qualified ISO-8601 value: {reason}")]
    InvalidFormat { reason: &'static str },
    #[error("timestamp has no UTC offset")]
    MissingOffset,
    #[error("timestamp uses the RFC 3339 unknown-local-offset marker")]
    UnknownOffset,
    #[error("timestamp time-zone annotations are not supported")]
    TimeZoneAnnotation,
    #[error("timestamp is outside the supported machine range")]
    OutOfRange,
}

/// An offset-qualified machine timestamp that preserves its recorded representation.
#[derive(Debug, Clone)]
pub struct MachineTimestamp {
    raw: String,
    instant: Timestamp,
    local: DateTime,
}

impl MachineTimestamp {
    fn parse(raw: String) -> Result<Self, TimestampError> {
        if raw.contains('[') || raw.contains(']') {
            return Err(TimestampError::TimeZoneAnnotation);
        }
        if !raw.is_ascii() {
            return Err(invalid_format("non-ASCII characters are not supported"));
        }

        let (datetime, offset) = split_recorded_offset(&raw)?;
        let local = parse_local_datetime(datetime)?;
        let instant = offset
            .to_timestamp(local)
            .map_err(|_| TimestampError::OutOfRange)?;
        Ok(Self {
            raw,
            instant,
            local,
        })
    }

    fn utc(instant: Timestamp) -> Self {
        let local = Offset::UTC.to_datetime(instant);
        Self {
            raw: format_utc(local),
            instant,
            local,
        }
    }

    /// Returns the calendar date in the timestamp's recorded offset.
    #[must_use]
    pub fn date(&self) -> String {
        format!(
            "{}-{:02}-{:02}",
            format_year(self.local.year()),
            self.local.month(),
            self.local.day()
        )
    }

    /// Returns the local date and minute in the timestamp's recorded offset.
    #[must_use]
    pub fn display_minute(&self) -> String {
        format!(
            "{}-{:02}-{:02} {:02}:{:02}",
            format_year(self.local.year()),
            self.local.month(),
            self.local.day(),
            self.local.hour(),
            self.local.minute()
        )
    }

    /// Returns the recorded instant, independent of its offset.
    #[must_use]
    pub const fn instant(&self) -> Timestamp {
        self.instant
    }

    /// Decodes Unix seconds and records the timestamp in UTC.
    pub fn from_unix_seconds(seconds: i64) -> Result<Self, TimestampError> {
        Timestamp::from_second(seconds)
            .map(Self::utc)
            .map_err(|_| TimestampError::OutOfRange)
    }
}

fn split_recorded_offset(raw: &str) -> Result<(&str, Offset), TimestampError> {
    let (_, time_and_offset) = raw
        .split_once('T')
        .ok_or_else(|| invalid_format("missing date-time separator"))?;
    if let Some(datetime) = raw.strip_suffix('Z') {
        return Ok((datetime, Offset::UTC));
    }

    let offset_start = time_and_offset
        .char_indices()
        .rev()
        .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index));
    let Some(offset_start) = offset_start else {
        parse_local_datetime(raw)?;
        return Err(TimestampError::MissingOffset);
    };
    let offset_start = raw.len() - time_and_offset.len() + offset_start;
    let (datetime, raw_offset) = raw.split_at(offset_start);
    Ok((datetime, parse_offset(raw_offset)?))
}

fn parse_local_datetime(raw: &str) -> Result<DateTime, TimestampError> {
    let (date, time) = raw
        .split_once('T')
        .ok_or_else(|| invalid_format("missing date-time separator"))?;
    if time.contains('T') {
        return Err(invalid_format("multiple date-time separators"));
    }

    let (year, month, day) = parse_date(date)?;
    let (whole_seconds, fraction) = time
        .char_indices()
        .find(|(_, character)| matches!(character, '.' | ','))
        .map_or((time, None), |(index, _)| {
            (&time[..index], Some(&time[index + 1..]))
        });
    let bytes = whole_seconds.as_bytes();
    if bytes.len() != 8 || bytes.get(2) != Some(&b':') || bytes.get(5) != Some(&b':') {
        return Err(invalid_format("invalid wall-clock time shape"));
    }

    let hour = parse_two_digits(&bytes[..2])?;
    let minute = parse_two_digits(&bytes[3..5])?;
    let second = parse_two_digits(&bytes[6..])?;
    let nanosecond = fraction.map_or(Ok(0), parse_fraction)?;
    DateTime::new(
        year,
        i8::try_from(month).map_err(|_| invalid_format("invalid month"))?,
        i8::try_from(day).map_err(|_| invalid_format("invalid day"))?,
        i8::try_from(hour).map_err(|_| invalid_format("invalid hour"))?,
        i8::try_from(minute).map_err(|_| invalid_format("invalid minute"))?,
        i8::try_from(second).map_err(|_| invalid_format("invalid second"))?,
        nanosecond,
    )
    .map_err(|_| invalid_format("invalid calendar date or wall-clock time"))
}

fn parse_date(raw: &str) -> Result<(i16, i32, i32), TimestampError> {
    let bytes = raw.as_bytes();
    let signed = matches!(bytes.first(), Some(b'+' | b'-'));
    let expected_length = if signed { 11 } else { 10 };
    let year_start = usize::from(signed);
    let year_end = year_start + 4;
    if bytes.len() != expected_length
        || bytes.get(year_end) != Some(&b'-')
        || bytes.get(year_end + 3) != Some(&b'-')
    {
        return Err(invalid_format("invalid calendar date shape"));
    }

    let mut year = parse_digits(&bytes[year_start..year_end], "invalid year")?;
    if bytes.first() == Some(&b'-') {
        year = -year;
    }
    let month = parse_two_digits(&bytes[year_end + 1..year_end + 3])?;
    let day = parse_two_digits(&bytes[year_end + 4..])?;
    Ok((
        i16::try_from(year).map_err(|_| invalid_format("invalid year"))?,
        month,
        day,
    ))
}

fn parse_offset(raw: &str) -> Result<Offset, TimestampError> {
    let bytes = raw.as_bytes();
    if !matches!(bytes.len(), 6 | 9)
        || !matches!(bytes.first(), Some(b'+' | b'-'))
        || bytes.get(3) != Some(&b':')
        || (bytes.len() == 9 && bytes.get(6) != Some(&b':'))
    {
        return Err(invalid_format("invalid UTC offset shape"));
    }

    let hours = parse_two_digits(&bytes[1..3])?;
    let minutes = parse_two_digits(&bytes[4..6])?;
    let seconds = if bytes.len() == 9 {
        parse_two_digits(&bytes[7..9])?
    } else {
        0
    };
    if hours > 25 || minutes > 59 || seconds > 59 {
        return Err(invalid_format("UTC offset is outside the supported range"));
    }
    if bytes.first() == Some(&b'-') && hours == 0 && minutes == 0 && seconds == 0 {
        return Err(TimestampError::UnknownOffset);
    }

    let total = hours * 3_600 + minutes * 60 + seconds;
    let signed = if bytes.first() == Some(&b'-') {
        -total
    } else {
        total
    };
    Offset::from_seconds(signed).map_err(|_| invalid_format("invalid UTC offset"))
}

fn parse_two_digits(raw: &[u8]) -> Result<i32, TimestampError> {
    parse_digits(raw, "invalid two-digit component")
}

fn parse_digits(raw: &[u8], reason: &'static str) -> Result<i32, TimestampError> {
    if raw.is_empty() || !raw.iter().all(u8::is_ascii_digit) {
        return Err(invalid_format(reason));
    }
    raw.iter().try_fold(0_i32, |value, byte| {
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i32::from(byte - b'0')))
            .ok_or(TimestampError::OutOfRange)
    })
}

fn parse_fraction(raw: &str) -> Result<i32, TimestampError> {
    if raw.is_empty() || raw.len() > 9 {
        return Err(invalid_format("invalid fractional second"));
    }
    let mut nanosecond = parse_digits(raw.as_bytes(), "invalid fractional second")?;
    for _ in raw.len()..9 {
        nanosecond *= 10;
    }
    Ok(nanosecond)
}

fn format_utc(local: DateTime) -> String {
    let prefix = format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
        format_year(local.year()),
        local.month(),
        local.day(),
        local.hour(),
        local.minute(),
        local.second()
    );
    let nanosecond = local.subsec_nanosecond();
    if nanosecond == 0 {
        format!("{prefix}Z")
    } else {
        let fraction = format!("{nanosecond:09}");
        format!("{prefix}.{}Z", fraction.trim_end_matches('0'))
    }
}

fn format_year(year: i16) -> String {
    if year < 0 {
        format!("-{:04}", year.unsigned_abs())
    } else {
        format!("{year:04}")
    }
}

fn invalid_format(reason: &'static str) -> TimestampError {
    TimestampError::InvalidFormat { reason }
}

impl AsRef<str> for MachineTimestamp {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for MachineTimestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}

impl PartialEq for MachineTimestamp {
    fn eq(&self, other: &Self) -> bool {
        self.instant == other.instant
    }
}

impl Eq for MachineTimestamp {}

impl PartialOrd for MachineTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MachineTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.instant.cmp(&other.instant)
    }
}

impl Serialize for MachineTimestamp {
    fn serialize<SerializerType>(
        &self,
        serializer: SerializerType,
    ) -> Result<SerializerType::Ok, SerializerType::Error>
    where
        SerializerType: Serializer,
    {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for MachineTimestamp {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(raw).map_err(DeserializerType::Error::custom)
    }
}

impl TryFrom<String> for MachineTimestamp {
    type Error = TimestampError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl TryFrom<&str> for MachineTimestamp {
    type Error = TimestampError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        Self::try_from(raw.to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TryFrom<std::time::SystemTime> for MachineTimestamp {
    type Error = TimestampError;

    fn try_from(system_time: std::time::SystemTime) -> Result<Self, Self::Error> {
        let instant = Timestamp::try_from(system_time).map_err(|_| TimestampError::OutOfRange)?;
        Ok(Self::utc(instant))
    }
}

impl FromStr for MachineTimestamp {
    type Err = TimestampError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::try_from(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_qualified_values_order_by_instant() {
        let earlier = MachineTimestamp::try_from("2026-01-01T00:30:00+01:00").unwrap();
        let later = MachineTimestamp::try_from("2025-12-31T23:45:00Z").unwrap();

        assert!(earlier < later);
    }

    #[test]
    fn timezone_less_and_annotated_values_are_rejected() {
        assert!(matches!(
            MachineTimestamp::try_from("2026-01-01T00:00:00"),
            Err(TimestampError::MissingOffset)
        ));
        assert!(matches!(
            MachineTimestamp::try_from("2026-01-01T00:00:00Z[UTC]"),
            Err(TimestampError::TimeZoneAnnotation)
        ));
    }

    #[test]
    fn unknown_offsets_and_invalid_calendar_values_are_rejected() {
        assert!(matches!(
            MachineTimestamp::try_from("2026-01-01T00:00:00-00:00"),
            Err(TimestampError::UnknownOffset)
        ));
        assert!(matches!(
            MachineTimestamp::try_from("2026-02-29T00:00:00Z"),
            Err(TimestampError::InvalidFormat { .. })
        ));
    }

    #[test]
    fn minute_display_is_derived_in_the_recorded_offset() {
        let timestamp = MachineTimestamp::try_from("2026-06-08T13:45:00-03:00").unwrap();

        assert_eq!(timestamp.display_minute(), "2026-06-08 13:45");
        assert_eq!(timestamp.as_ref(), "2026-06-08T13:45:00-03:00");
    }

    #[test]
    fn serde_preserves_the_established_machine_string() {
        let timestamp = MachineTimestamp::try_from("2026-08-09T10:00:00Z").unwrap();
        let json = serde_json::to_string(&timestamp).unwrap();

        assert_eq!(json, r#""2026-08-09T10:00:00Z""#);
        assert_eq!(
            serde_json::from_str::<MachineTimestamp>(&json).unwrap(),
            timestamp
        );
    }

    #[test]
    fn unix_seconds_construct_ordered_utc_timestamps() {
        let earlier = MachineTimestamp::from_unix_seconds(100).unwrap();
        let later = MachineTimestamp::from_unix_seconds(200).unwrap();

        assert!(earlier < later);
        assert_eq!(earlier.as_ref(), "1970-01-01T00:01:40Z");
    }

    #[test]
    fn parser_uses_jiff_instants() {
        for raw in [
            "1969-12-31T23:59:59.5Z",
            "1970-01-01T00:00:00Z",
            "2000-02-29T12:34:56.123456789+14:00",
            "2026-08-17T23:59:59-03:00",
        ] {
            let expected = raw.parse::<Timestamp>().unwrap();
            let actual = MachineTimestamp::try_from(raw).unwrap();

            assert_eq!(actual.instant, expected, "{raw}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_clock_conversion_derives_utc_calendar_fields() {
        let system_time = std::time::UNIX_EPOCH - std::time::Duration::from_millis(500);
        let timestamp = MachineTimestamp::try_from(system_time).unwrap();

        assert_eq!(timestamp.as_ref(), "1969-12-31T23:59:59.5Z");
        assert_eq!(timestamp.date(), "1969-12-31");
    }
}
