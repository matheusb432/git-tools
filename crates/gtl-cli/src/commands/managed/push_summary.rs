//! CLI-owned summary formatting for managed and recursive push commands.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushOutcome {
    Pushed,
    UpToDate,
    Skipped,
    Failed,
    Warned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PushSummary {
    total: usize,
    pushed: usize,
    up_to_date: usize,
    skipped: usize,
    failed: usize,
    warned: usize,
    excluded: usize,
    dry: bool,
}

impl PushSummary {
    pub(crate) fn from_outcomes(
        outcomes: impl IntoIterator<Item = PushOutcome>,
        dry: bool,
        excluded: usize,
    ) -> Self {
        let mut summary = Self {
            total: excluded,
            pushed: 0,
            up_to_date: 0,
            skipped: 0,
            failed: 0,
            warned: 0,
            excluded,
            dry,
        };
        for outcome in outcomes {
            summary.total += 1;
            match outcome {
                PushOutcome::Pushed => summary.pushed += 1,
                PushOutcome::UpToDate => summary.up_to_date += 1,
                PushOutcome::Skipped => summary.skipped += 1,
                PushOutcome::Failed => summary.failed += 1,
                PushOutcome::Warned => summary.warned += 1,
            }
        }
        summary
    }

    pub(crate) fn render(self) -> String {
        let pushed_label = if self.dry { "would push" } else { "pushed" };
        let total = crate::output::count_label(self.total, "project", "projects");
        let counts = [
            (self.pushed, pushed_label),
            (self.up_to_date, "up to date"),
            (self.skipped, "skipped"),
            (self.failed, "failed"),
            (
                self.warned,
                if self.warned == 1 {
                    "warning"
                } else {
                    "warnings"
                },
            ),
            (self.excluded, "excluded"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, label)| format!("{count} {label}"))
        .collect::<Vec<_>>();
        if counts.is_empty() {
            total
        } else {
            format!("{total}: {}", counts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_summary_reports_pushed_and_skipped() {
        let summary = PushSummary::from_outcomes(
            [
                PushOutcome::Pushed,
                PushOutcome::Pushed,
                PushOutcome::Skipped,
            ],
            false,
            0,
        );
        assert_eq!(summary.pushed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.render(), "3 projects: 2 pushed, 1 skipped");
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
            0,
        );
        assert_eq!(
            summary.render(),
            "4 projects: 1 pushed, 1 skipped, 1 failed, 1 warning"
        );
    }

    #[test]
    fn failed_only_summary_omits_zero_warn_count() {
        let summary = PushSummary::from_outcomes([PushOutcome::Failed], false, 0);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.render(), "1 project: 1 failed");
        assert!(!summary.render().contains("warn"));
    }

    #[test]
    fn warned_only_summary_omits_zero_fail_count() {
        let summary = PushSummary::from_outcomes([PushOutcome::Warned], false, 0);
        assert_eq!(summary.render(), "1 project: 1 warning");
        assert!(!summary.render().contains("fail"));
    }

    #[test]
    fn dry_summary_omits_zero_counts() {
        let summary = PushSummary::from_outcomes([PushOutcome::Skipped], true, 0);
        assert_eq!(summary.render(), "1 project: 1 skipped");
    }

    #[test]
    fn excluded_repositories_extend_the_total_and_append_a_distinct_count() {
        let summary = PushSummary::from_outcomes([PushOutcome::Pushed], true, 2);

        assert_eq!(summary.render(), "3 projects: 1 would push, 2 excluded");
    }
}
