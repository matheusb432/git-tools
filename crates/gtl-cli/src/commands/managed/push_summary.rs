//! CLI-owned summary formatting for managed and recursive push commands.

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushOutcome {
    Pushed,
    Skipped,
    Failed,
    Warned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PushSummary {
    total: usize,
    pushed: usize,
    skipped: usize,
    failed: usize,
    warned: usize,
    dry: bool,
}

impl PushSummary {
    pub(crate) fn from_outcomes(
        outcomes: impl IntoIterator<Item = PushOutcome>,
        dry: bool,
    ) -> Self {
        let mut summary = Self {
            total: 0,
            pushed: 0,
            skipped: 0,
            failed: 0,
            warned: 0,
            dry,
        };
        for outcome in outcomes {
            summary.total += 1;
            match outcome {
                PushOutcome::Pushed => summary.pushed += 1,
                PushOutcome::Skipped => summary.skipped += 1,
                PushOutcome::Failed => summary.failed += 1,
                PushOutcome::Warned => summary.warned += 1,
            }
        }
        summary
    }

    pub(crate) fn render(self, exit_code: i32) -> String {
        let pushed_label = if self.dry { "would push" } else { "pushed" };
        let mut output = format!(
            "exit {exit_code}  -  {} repos: {} {pushed_label}, {} skipped",
            self.total, self.pushed, self.skipped
        );
        if self.failed > 0 {
            let _ = write!(output, ", {} fail", self.failed);
        }
        if self.warned > 0 {
            let _ = write!(output, ", {} warn", self.warned);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_summary_always_reports_pushed_and_skipped() {
        let summary = PushSummary::from_outcomes(
            [
                PushOutcome::Pushed,
                PushOutcome::Pushed,
                PushOutcome::Skipped,
            ],
            false,
        );
        assert_eq!(summary.pushed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.render(0), "exit 0  -  3 repos: 2 pushed, 1 skipped");
    }

    #[test]
    fn mixed_summary_appends_only_nonzero_fail_and_warn_counts() {
        let summary = PushSummary::from_outcomes(
            [
                PushOutcome::Pushed,
                PushOutcome::Skipped,
                PushOutcome::Failed,
                PushOutcome::Warned,
            ],
            false,
        );
        assert_eq!(
            summary.render(1),
            "exit 1  -  4 repos: 1 pushed, 1 skipped, 1 fail, 1 warn"
        );
    }

    #[test]
    fn failed_only_summary_omits_zero_warn_count() {
        let summary = PushSummary::from_outcomes([PushOutcome::Failed], false);
        assert_eq!(summary.failed, 1);
        assert_eq!(
            summary.render(1),
            "exit 1  -  1 repos: 0 pushed, 0 skipped, 1 fail"
        );
        assert!(!summary.render(1).contains("warn"));
    }

    #[test]
    fn warned_only_summary_omits_zero_fail_count() {
        let summary = PushSummary::from_outcomes([PushOutcome::Warned], false);
        assert_eq!(
            summary.render(1),
            "exit 1  -  1 repos: 0 pushed, 0 skipped, 1 warn"
        );
        assert!(!summary.render(1).contains("fail"));
    }

    #[test]
    fn dry_summary_uses_would_push_and_keeps_zero_pushed_visible() {
        let summary = PushSummary::from_outcomes([PushOutcome::Skipped], true);
        assert_eq!(
            summary.render(0),
            "exit 0  -  1 repos: 0 would push, 1 skipped"
        );
    }
}
