use crate::shared::viewer_client::ViewerClientError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveError {
    pub error: ViewerClientError,
    pub occurrences: u64,
    pub elapsed_ms_last: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LiveErrors {
    entries: Vec<LiveError>,
    recovery_started_ms: Option<u64>,
}

impl LiveErrors {
    pub(crate) fn entries(&self) -> &[LiveError] {
        &self.entries
    }

    pub(crate) fn interrupt(&mut self) {
        self.recovery_started_ms = None;
    }

    pub(crate) fn observe(&mut self, result: Result<(), ViewerClientError>, elapsed_ms: u64) {
        match result {
            Err(error) => self.record_failure(error, elapsed_ms),
            Ok(()) => self.record_success(elapsed_ms),
        }
    }

    fn record_failure(&mut self, error: ViewerClientError, elapsed_ms: u64) {
        self.interrupt();
        let occurrences = self
            .entries
            .iter()
            .position(|entry| entry.error == error)
            .map_or(1, |index| {
                self.entries.remove(index).occurrences.saturating_add(1)
            });
        self.entries.insert(
            0,
            LiveError {
                error,
                occurrences,
                elapsed_ms_last: elapsed_ms,
            },
        );
        self.entries.truncate(5);
    }

    fn record_success(&mut self, elapsed_ms: u64) {
        if self.entries.is_empty() {
            return;
        }
        let started = self.recovery_started_ms.get_or_insert(elapsed_ms);
        if elapsed_ms.saturating_sub(*started) >= 60_000 {
            self.entries.clear();
            self.interrupt();
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::Failure;

    use super::*;

    /// Builds a distinct error for each label.
    fn failure(label: &str) -> ViewerClientError {
        Failure::InvalidRequest {
            field: label.to_owned(),
        }
        .into()
    }

    #[test]
    fn recent_distinct_errors_are_bounded_and_repeats_move_to_front() {
        let mut errors = LiveErrors::default();
        for index in 0..7 {
            errors.observe(Err(failure(&format!("failure {index}"))), index);
        }
        errors.observe(Err(failure("failure 3")), 10);
        assert_eq!(errors.entries().len(), 5);
        assert_eq!(errors.entries()[0].error, failure("failure 3"));
        assert_eq!(errors.entries()[0].occurrences, 2);
        assert_eq!(errors.entries()[0].elapsed_ms_last, 10);
    }

    #[test]
    fn cleanup_requires_a_minute_of_confirmed_recovery() {
        let mut errors = LiveErrors::default();
        errors.observe(Err(failure("failed")), 0);
        errors.observe(Ok(()), 100_000);
        errors.observe(Ok(()), 159_999);
        assert_eq!(errors.entries().len(), 1);
        errors.observe(Ok(()), 160_000);
        assert_eq!(errors.entries(), []);
    }

    #[test]
    fn failure_or_interruption_restarts_recovery() {
        let mut errors = LiveErrors::default();
        errors.observe(Err(failure("failed")), 0);
        errors.observe(Ok(()), 2_000);
        errors.observe(Err(failure("failed again")), 59_000);
        errors.observe(Ok(()), 62_000);
        assert_eq!(errors.entries().len(), 2);
        errors.interrupt();
        errors.observe(Ok(()), 200_000);
        assert_eq!(errors.entries().len(), 2);
        errors.observe(Ok(()), 260_000);
        assert_eq!(errors.entries(), []);
    }
}
