//! The system [`Clock`] adapter.

use std::time::SystemTime;

use gtl_application::ports::Clock;
use gtl_models::timestamps::MachineTimestamp;

/// Wall-clock time from the host.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<MachineTimestamp, gtl_models::timestamps::TimestampError> {
        SystemTime::now().try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_a_validated_utc_machine_timestamp() {
        let timestamp = SystemClock.now().expect("current system time is supported");

        assert!(timestamp.as_ref().ends_with('Z'));
    }
}
