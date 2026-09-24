use std::time::Duration;

use web_sys::Element;

/// Longest computed animation allowed to delay an element's removal.
const ANIMATION_DURATION_MAX: Duration = Duration::from_secs(1);

/// Returns the element's first computed animation duration.
///
/// Returns zero when the effective style disables animation, including reduced motion. Returns
/// `None` when the style is unavailable or the duration exceeds [`ANIMATION_DURATION_MAX`].
pub(super) fn computed_animation_duration(element: &Element) -> Option<Duration> {
    let style = web_sys::window()?
        .get_computed_style(element)
        .ok()
        .flatten()?;
    if style
        .get_property_value("animation-name")
        .is_ok_and(|name| name.trim() == "none")
    {
        return Some(Duration::ZERO);
    }
    style
        .get_property_value("animation-duration")
        .ok()
        .and_then(|value| parse_css_duration(&value))
}

fn parse_css_duration(value: &str) -> Option<Duration> {
    let value = value.split(',').next()?.trim();
    let seconds = match value.strip_suffix("ms") {
        Some(milliseconds) => milliseconds.trim().parse::<f64>().ok()? / 1_000.0,
        None => value.strip_suffix('s')?.trim().parse::<f64>().ok()?,
    };
    (seconds.is_finite() && seconds >= 0.0 && seconds <= ANIMATION_DURATION_MAX.as_secs_f64())
        .then(|| Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::parse_css_duration;

    #[test]
    fn css_duration_accepts_seconds_and_milliseconds() {
        assert_eq!(
            parse_css_duration("120ms"),
            Some(Duration::from_millis(120))
        );
        assert_eq!(
            parse_css_duration("0.16s"),
            Some(Duration::from_millis(160))
        );
        assert_eq!(
            parse_css_duration("0.12s, 80ms"),
            Some(Duration::from_millis(120))
        );
    }

    #[test]
    fn css_duration_rejects_invalid_values() {
        assert_eq!(parse_css_duration("none"), None);
        assert_eq!(parse_css_duration("-120ms"), None);
        assert_eq!(parse_css_duration("NaNs"), None);
        assert_eq!(parse_css_duration("1e300s"), None);
    }
}
