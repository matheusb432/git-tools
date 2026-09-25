//! Displays timestamps in the viewer's date format and local time zone.
//!
//! The desktop viewer provides its configured format through
//! [`use_date_display_provider`]. Trees without a provider, such as offline
//! artifacts, keep ISO in each timestamp's recorded offset.

use dioxus::prelude::*;
#[cfg(feature = "interactive-ui")]
use gtl_models::settings::{ViewerDateFormat, ViewerLanguage};
use gtl_models::timestamps::MachineTimestamp;
#[cfg(feature = "interactive-ui")]
use jiff::civil::DateTime;

#[cfg(feature = "interactive-ui")]
use crate::shared::i18n::t;

#[cfg(feature = "interactive-ui")]
const SECONDS_PER_MINUTE: i64 = 60;
#[cfg(feature = "interactive-ui")]
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
#[cfg(feature = "interactive-ui")]
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
/// Dates at least this old show ISO in the relative format.
#[cfg(feature = "interactive-ui")]
const RELATIVE_AGE_SECONDS_LIMIT: i64 = 7 * SECONDS_PER_DAY;
/// How often relative dates advance.
#[cfg(feature = "desktop")]
const RELATIVE_CLOCK_PERIOD: std::time::Duration = std::time::Duration::from_secs(60);

/// The date the settings options format as an example. Its day exceeds 12,
/// so the day-first and month-first samples cannot be confused.
#[cfg(feature = "interactive-ui")]
const SAMPLE_DATE_TIME: DateTime = jiff::civil::date(2026, 6, 28).at(13, 45, 0, 0);
/// The age the relative settings option formats as an example.
#[cfg(feature = "interactive-ui")]
const SAMPLE_AGE_SECONDS: i64 = 3 * SECONDS_PER_HOUR;

/// Shows `timestamp` in a `time` element whose tooltip holds the exact time.
#[component]
pub(crate) fn DateDisplayTime(
    timestamp: MachineTimestamp,
    #[props(default)] class: String,
) -> Element {
    #[cfg(feature = "desktop")]
    let DateDisplayText { text, exact, .. } =
        use_date_display().show(&timestamp, crate::shared::i18n::use_language());
    #[cfg(not(feature = "desktop"))]
    let (text, exact) = (timestamp.display_minute(), timestamp.to_string());
    rsx! {
        time { class, datetime: timestamp.to_string(), title: exact, "{text}" }
    }
}

/// A timestamp formatted for display.
#[cfg(feature = "desktop")]
pub(crate) struct DateDisplayText {
    /// The date in the configured format.
    pub(crate) text: String,
    /// The exact time and its offset, for a tooltip.
    pub(crate) exact: String,
    /// Whether `text` states an age, such as `3 hours ago`, instead of a date.
    pub(crate) relative: bool,
}

/// Formats timestamps for the component that called [`use_date_display`].
#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(crate) enum DateDisplay {
    /// ISO in each timestamp's recorded offset, as offline artifacts show it.
    Recorded,
    /// The configured format in the local time zone.
    Viewer(ViewerDateDisplay),
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(crate) struct ViewerDateDisplay {
    format: ReadSignal<ViewerDateFormat>,
    now: ReadSignal<jiff::Timestamp>,
}

/// Makes `format` the date format of every descendant's [`use_date_display`].
#[cfg(feature = "desktop")]
pub(crate) fn use_date_display_provider(format: ReadSignal<ViewerDateFormat>) {
    let mut now = use_signal(crate::shared::browser::current_timestamp);
    // Only relative dates read `now`, so other formats never re-render on a tick.
    use_future(move || async move {
        loop {
            dioxus_sdk_time::sleep(RELATIVE_CLOCK_PERIOD).await;
            now.set(crate::shared::browser::current_timestamp());
        }
    });
    use_context_provider(|| ViewerDateDisplay {
        format,
        now: now.into(),
    });
}

/// Returns the date display of the calling component's tree.
#[cfg(feature = "desktop")]
pub(crate) fn use_date_display() -> DateDisplay {
    try_use_context::<ViewerDateDisplay>().map_or(DateDisplay::Recorded, DateDisplay::Viewer)
}

#[cfg(feature = "desktop")]
impl DateDisplay {
    /// Formats `timestamp`, subscribing the calling component to the format
    /// and, for relative dates, to the clock.
    pub(crate) fn show(
        self,
        timestamp: &MachineTimestamp,
        language: ViewerLanguage,
    ) -> DateDisplayText {
        let Self::Viewer(viewer) = self else {
            return DateDisplayText {
                text: timestamp.display_minute(),
                exact: timestamp.to_string(),
                relative: false,
            };
        };
        let instant = timestamp.instant();
        let local = instant.to_zoned(jiff::tz::TimeZone::fixed(
            crate::shared::browser::local_offset_at(instant),
        ));
        let exact = local.strftime("%Y-%m-%d %H:%M:%S %:z").to_string();
        let format = (viewer.format)();
        let relative = (format == ViewerDateFormat::Relative)
            .then(|| {
                let age_seconds = (viewer.now)().as_second() - instant.as_second();
                relative_date_text(age_seconds, language)
            })
            .flatten();
        match relative {
            Some(text) => DateDisplayText {
                text,
                exact,
                relative: true,
            },
            None => DateDisplayText {
                text: absolute_date_text(local.datetime(), format),
                exact,
                relative: false,
            },
        }
    }
}

/// Formats the settings example of `format`.
#[cfg(feature = "interactive-ui")]
pub(crate) fn date_format_sample(format: ViewerDateFormat, language: ViewerLanguage) -> String {
    match format {
        ViewerDateFormat::Relative => relative_date_text(SAMPLE_AGE_SECONDS, language)
            .unwrap_or_else(|| absolute_date_text(SAMPLE_DATE_TIME, format)),
        ViewerDateFormat::Iso | ViewerDateFormat::DayFirst | ViewerDateFormat::MonthFirst => {
            absolute_date_text(SAMPLE_DATE_TIME, format)
        }
    }
}

/// Formats `local` to the minute; the relative format uses ISO, its format for older dates.
#[cfg(feature = "interactive-ui")]
fn absolute_date_text(local: DateTime, format: ViewerDateFormat) -> String {
    let pattern = match format {
        ViewerDateFormat::Iso | ViewerDateFormat::Relative => "%Y-%m-%d %H:%M",
        ViewerDateFormat::DayFirst => "%d/%m/%Y %H:%M",
        ViewerDateFormat::MonthFirst => "%m/%d/%Y %-I:%M %p",
    };
    local.strftime(pattern).to_string()
}

/// States an age of `age_seconds`, or `None` for future dates and dates a week or older.
#[cfg(feature = "interactive-ui")]
fn relative_date_text(age_seconds: i64, language: ViewerLanguage) -> Option<String> {
    Some(match age_seconds {
        0..SECONDS_PER_MINUTE => t!(language, "date-just-now"),
        SECONDS_PER_MINUTE..SECONDS_PER_HOUR => t!(
            language,
            "date-minutes-ago",
            count = age_seconds / SECONDS_PER_MINUTE
        ),
        SECONDS_PER_HOUR..SECONDS_PER_DAY => t!(
            language,
            "date-hours-ago",
            count = age_seconds / SECONDS_PER_HOUR
        ),
        SECONDS_PER_DAY..RELATIVE_AGE_SECONDS_LIMIT => t!(
            language,
            "date-days-ago",
            count = age_seconds / SECONDS_PER_DAY
        ),
        _ => return None,
    })
}

#[cfg(all(test, feature = "interactive-ui"))]
mod tests {
    use super::*;

    #[test]
    fn absolute_formats_order_fields_by_convention() {
        let afternoon = jiff::civil::date(2026, 6, 28).at(13, 5, 59, 0);
        let midnight = jiff::civil::date(2026, 1, 2).at(0, 7, 0, 0);

        assert_eq!(
            absolute_date_text(afternoon, ViewerDateFormat::Iso),
            "2026-06-28 13:05"
        );
        assert_eq!(
            absolute_date_text(afternoon, ViewerDateFormat::DayFirst),
            "28/06/2026 13:05"
        );
        assert_eq!(
            absolute_date_text(afternoon, ViewerDateFormat::MonthFirst),
            "06/28/2026 1:05 PM"
        );
        assert_eq!(
            absolute_date_text(midnight, ViewerDateFormat::MonthFirst),
            "01/02/2026 12:07 AM"
        );
        assert_eq!(
            absolute_date_text(afternoon, ViewerDateFormat::Relative),
            "2026-06-28 13:05"
        );
    }

    #[test]
    fn relative_ages_use_the_largest_whole_unit_within_a_week() {
        let english = |age_seconds| relative_date_text(age_seconds, ViewerLanguage::EnUs);

        assert_eq!(english(0).as_deref(), Some("just now"));
        assert_eq!(english(59).as_deref(), Some("just now"));
        assert_eq!(english(60).as_deref(), Some("1 minute ago"));
        assert_eq!(
            english(SECONDS_PER_HOUR - 1).as_deref(),
            Some("59 minutes ago")
        );
        assert_eq!(english(SECONDS_PER_HOUR).as_deref(), Some("1 hour ago"));
        assert_eq!(
            english(SECONDS_PER_DAY - 1).as_deref(),
            Some("23 hours ago")
        );
        assert_eq!(english(SECONDS_PER_DAY).as_deref(), Some("1 day ago"));
        assert_eq!(
            english(RELATIVE_AGE_SECONDS_LIMIT - 1).as_deref(),
            Some("6 days ago")
        );
        assert_eq!(english(RELATIVE_AGE_SECONDS_LIMIT), None);
        assert_eq!(english(-1), None);
        assert_eq!(
            relative_date_text(2 * SECONDS_PER_HOUR, ViewerLanguage::PtBr).as_deref(),
            Some("há 2 horas")
        );
    }

    #[test]
    fn samples_distinguish_day_and_month_order() {
        let samples = ViewerDateFormat::ALL
            .iter()
            .map(|format| date_format_sample(*format, ViewerLanguage::EnUs))
            .collect::<Vec<_>>();

        assert_eq!(
            samples,
            [
                "2026-06-28 13:45",
                "28/06/2026 13:45",
                "06/28/2026 1:45 PM",
                "3 hours ago"
            ]
        );
    }
}
