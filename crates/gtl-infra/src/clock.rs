//! The system [`Clock`] adapter.

use gtl_application::ports::Clock;

/// Wall-clock time from the host, formatted exactly as the store's `generated_at`
/// field has always been: `jiff::Timestamp::now().to_string()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_iso(&self) -> String {
        jiff::Timestamp::now().to_string()
    }
}
