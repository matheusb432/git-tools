use std::time::Duration;

const FIRST_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetryDelay {
    next: Duration,
}

impl Default for RetryDelay {
    fn default() -> Self {
        Self {
            next: FIRST_RETRY_DELAY,
        }
    }
}

impl RetryDelay {
    pub(crate) fn take_and_advance(&mut self) -> Duration {
        let current = self.next;
        self.next = self.next.saturating_mul(2).min(MAX_RETRY_DELAY);
        current
    }

    pub(crate) fn reset(&mut self) {
        self.next = FIRST_RETRY_DELAY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_starts_at_250_milliseconds_and_stops_growing_at_five_seconds() {
        let mut delay = RetryDelay::default();

        assert_eq!(delay.take_and_advance(), Duration::from_millis(250));
        assert_eq!(delay.take_and_advance(), Duration::from_millis(500));
        assert_eq!(delay.take_and_advance(), Duration::from_secs(1));
        assert_eq!(delay.take_and_advance(), Duration::from_secs(2));
        assert_eq!(delay.take_and_advance(), Duration::from_secs(4));
        assert_eq!(delay.take_and_advance(), Duration::from_secs(5));
        assert_eq!(delay.take_and_advance(), Duration::from_secs(5));
    }

    #[test]
    fn successful_connection_resets_the_delay() {
        let mut delay = RetryDelay::default();
        let _ = delay.take_and_advance();
        let _ = delay.take_and_advance();

        delay.reset();

        assert_eq!(delay.take_and_advance(), Duration::from_millis(250));
    }
}
