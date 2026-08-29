use super::{BENCHMARK_NAME, DesktopScrollReport, REPORT_FORMAT_VERSION, ScrollSample};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricDelta {
    pub baseline: f64,
    pub current: f64,
    /// Absent when the baseline is zero.
    pub relative_change_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelComparison {
    pub frame_gap_ms: FrameGapComparison,
    pub frames_exceeding_33_ms: MetricDelta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameGapComparison {
    pub p50: MetricDelta,
    pub p95: MetricDelta,
    pub p99: MetricDelta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopScrollComparison {
    pub readiness_wall_time_milliseconds: MetricDelta,
    pub readiness_process_cpu_time_milliseconds: MetricDelta,
    pub changed_files: PanelComparison,
    pub commits: PanelComparison,
    pub peak_rss_bytes: MetricDelta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopScrollReportRole {
    Baseline,
    Current,
}

impl std::fmt::Display for DesktopScrollReportRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Baseline => formatter.write_str("baseline"),
            Self::Current => formatter.write_str("current"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopScrollPanel {
    ChangedFiles,
    Commits,
}

impl DesktopScrollPanel {
    const fn name(self) -> &'static str {
        match self {
            Self::ChangedFiles => "changed-files",
            Self::Commits => "commits",
        }
    }

    fn sample(self, launch: &super::DesktopScrollLaunch) -> &ScrollSample {
        match self {
            Self::ChangedFiles => &launch.changed_files,
            Self::Commits => &launch.commits,
        }
    }
}

impl std::fmt::Display for DesktopScrollPanel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum DesktopScrollComparisonError {
    #[error("{report} desktop scroll report uses format version {actual}, expected {expected}")]
    UnsupportedFormat {
        report: DesktopScrollReportRole,
        actual: u32,
        expected: u32,
    },
    #[error("{report} desktop scroll report has benchmark identity {actual}")]
    UnexpectedBenchmark {
        report: DesktopScrollReportRole,
        actual: String,
    },
    #[error("{report} desktop scroll report has no launches")]
    MissingLaunches { report: DesktopScrollReportRole },
    #[error("{report} desktop scroll report has a zero process CPU clock frequency")]
    InvalidProcessCpuClockFrequency { report: DesktopScrollReportRole },
    #[error(
        "{report} desktop scroll report contains {actual} launches but its protocol records {expected}"
    )]
    LaunchCountMismatch {
        report: DesktopScrollReportRole,
        expected: usize,
        actual: usize,
    },
    #[error(
        "{report} desktop scroll report expected launch {expected} but recorded launch {actual}"
    )]
    LaunchOutOfSequence {
        report: DesktopScrollReportRole,
        expected: usize,
        actual: usize,
    },
    #[error("{report} desktop scroll launch {launch} {panel} sample has panel identity {actual}")]
    PanelIdentityMismatch {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
        actual: String,
    },
    #[error("{report} desktop scroll launch {launch} {panel} sample uses another scroll protocol")]
    ScrollProtocolMismatch {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
    },
    #[error("{report} desktop scroll launch {launch} {panel} sample has no frame gaps")]
    MissingFrameGaps {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
    },
    #[error(
        "{report} desktop scroll launch {launch} {panel} frame counts do not match its raw gaps"
    )]
    FrameCountMismatch {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
    },
    #[error(
        "{report} desktop scroll launch {launch} {panel} sample contains an invalid frame gap at index {index}"
    )]
    InvalidFrameGap {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
        index: usize,
    },
    #[error(
        "{report} desktop scroll launch {launch} {panel} slow-frame count does not match its raw gaps"
    )]
    SlowFrameCountMismatch {
        report: DesktopScrollReportRole,
        launch: usize,
        panel: DesktopScrollPanel,
    },
    #[error("{report} desktop scroll launch {launch} process memory uses another attribution")]
    MemoryAttributionMismatch {
        report: DesktopScrollReportRole,
        launch: usize,
    },
    #[error("desktop scroll reports use different fixtures")]
    IncompatibleFixture,
    #[error("desktop scroll reports use different protocols")]
    IncompatibleProtocol,
    #[error("desktop scroll reports use different resource bounds")]
    IncompatibleResourceBounds,
    #[error("desktop scroll reports use different runner environments")]
    IncompatibleRunnerEnvironment,
    #[error("desktop scroll reports use different build profiles")]
    IncompatibleBuildProfile,
}

#[derive(Clone, Copy)]
struct PanelMetrics {
    frame_gap_ms: FrameGapMetrics,
    frames_exceeding_33_ms: u64,
}

#[derive(Clone, Copy)]
struct FrameGapMetrics {
    p50: f64,
    p95: f64,
    p99: f64,
}

/// Compares compatible reports by combining launch distributions and peak memory observations.
///
/// # Errors
///
/// Returns [`DesktopScrollComparisonError`] when either report is invalid or their measurement
/// environments are incompatible.
pub fn compare_reports(
    baseline: &DesktopScrollReport,
    current: &DesktopScrollReport,
) -> Result<DesktopScrollComparison, DesktopScrollComparisonError> {
    validate_report(DesktopScrollReportRole::Baseline, baseline)?;
    validate_report(DesktopScrollReportRole::Current, current)?;
    if baseline.fixture_manifest != current.fixture_manifest {
        return Err(DesktopScrollComparisonError::IncompatibleFixture);
    }
    if baseline.protocol != current.protocol {
        return Err(DesktopScrollComparisonError::IncompatibleProtocol);
    }
    if baseline.resource_bounds != current.resource_bounds {
        return Err(DesktopScrollComparisonError::IncompatibleResourceBounds);
    }
    if baseline.runner != current.runner {
        return Err(DesktopScrollComparisonError::IncompatibleRunnerEnvironment);
    }
    if baseline.source.profile != current.source.profile {
        return Err(DesktopScrollComparisonError::IncompatibleBuildProfile);
    }

    let baseline_changed_files = panel_metrics(baseline, DesktopScrollPanel::ChangedFiles);
    let current_changed_files = panel_metrics(current, DesktopScrollPanel::ChangedFiles);
    let baseline_commits = panel_metrics(baseline, DesktopScrollPanel::Commits);
    let current_commits = panel_metrics(current, DesktopScrollPanel::Commits);

    Ok(DesktopScrollComparison {
        readiness_wall_time_milliseconds: MetricDelta::between(
            median_u64_as_f64(
                baseline
                    .launches
                    .iter()
                    .map(|launch| launch.readiness.wall_time_milliseconds),
            ),
            median_u64_as_f64(
                current
                    .launches
                    .iter()
                    .map(|launch| launch.readiness.wall_time_milliseconds),
            ),
        ),
        readiness_process_cpu_time_milliseconds: MetricDelta::between(
            process_cpu_time_milliseconds(baseline),
            process_cpu_time_milliseconds(current),
        ),
        changed_files: compare_panels(baseline_changed_files, current_changed_files),
        commits: compare_panels(baseline_commits, current_commits),
        peak_rss_bytes: MetricDelta::between(
            u64_to_f64(peak_rss_bytes(baseline)),
            u64_to_f64(peak_rss_bytes(current)),
        ),
    })
}

impl MetricDelta {
    fn between(baseline: f64, current: f64) -> Self {
        let relative_change_percent = if baseline == 0.0 {
            None
        } else {
            Some((current - baseline) / baseline * 100.0)
        };
        Self {
            baseline,
            current,
            relative_change_percent,
        }
    }
}

fn validate_report(
    report_role: DesktopScrollReportRole,
    report: &DesktopScrollReport,
) -> Result<(), DesktopScrollComparisonError> {
    if report.format_version != REPORT_FORMAT_VERSION {
        return Err(DesktopScrollComparisonError::UnsupportedFormat {
            report: report_role,
            actual: report.format_version,
            expected: REPORT_FORMAT_VERSION,
        });
    }
    if report.benchmark != BENCHMARK_NAME {
        return Err(DesktopScrollComparisonError::UnexpectedBenchmark {
            report: report_role,
            actual: report.benchmark.clone(),
        });
    }
    if report.launches.is_empty() {
        return Err(DesktopScrollComparisonError::MissingLaunches {
            report: report_role,
        });
    }
    if report.protocol.process_cpu_clock_ticks_per_second == 0 {
        return Err(
            DesktopScrollComparisonError::InvalidProcessCpuClockFrequency {
                report: report_role,
            },
        );
    }
    if report.launches.len() != report.protocol.independent_launches {
        return Err(DesktopScrollComparisonError::LaunchCountMismatch {
            report: report_role,
            expected: report.protocol.independent_launches,
            actual: report.launches.len(),
        });
    }

    for (launch_index, launch) in report.launches.iter().enumerate() {
        let expected_launch = launch_index + 1;
        if launch.launch != expected_launch {
            return Err(DesktopScrollComparisonError::LaunchOutOfSequence {
                report: report_role,
                expected: expected_launch,
                actual: launch.launch,
            });
        }
        let memory_snapshots = [
            &launch.readiness.peak_memory,
            &launch.memory_after_changed_files,
            &launch.memory_after_commits,
        ];
        if memory_snapshots
            .iter()
            .any(|memory| memory.attribution != report.protocol.memory_attribution)
        {
            return Err(DesktopScrollComparisonError::MemoryAttributionMismatch {
                report: report_role,
                launch: launch.launch,
            });
        }
        validate_sample(
            report_role,
            report,
            DesktopScrollPanel::ChangedFiles,
            launch,
        )?;
        validate_sample(report_role, report, DesktopScrollPanel::Commits, launch)?;
    }
    Ok(())
}

fn validate_sample(
    report_role: DesktopScrollReportRole,
    report: &DesktopScrollReport,
    panel: DesktopScrollPanel,
    launch: &super::DesktopScrollLaunch,
) -> Result<(), DesktopScrollComparisonError> {
    let sample = panel.sample(launch);
    if sample.panel != panel.name() {
        return Err(DesktopScrollComparisonError::PanelIdentityMismatch {
            report: report_role,
            launch: launch.launch,
            panel,
            actual: sample.panel.clone(),
        });
    }
    if sample.distance_css_pixels != report.protocol.scroll.distance_css_pixels
        || sample.step_css_pixels != report.protocol.scroll.step_css_pixels
        || sample.traversals != report.protocol.scroll.traversals
    {
        return Err(DesktopScrollComparisonError::ScrollProtocolMismatch {
            report: report_role,
            launch: launch.launch,
            panel,
        });
    }
    if sample.frame_gaps_ms.is_empty() {
        return Err(DesktopScrollComparisonError::MissingFrameGaps {
            report: report_role,
            launch: launch.launch,
            panel,
        });
    }
    if sample.frame_gap_count != sample.frame_gaps_ms.len()
        || sample.frame_count != sample.frame_gaps_ms.len() + 1
    {
        return Err(DesktopScrollComparisonError::FrameCountMismatch {
            report: report_role,
            launch: launch.launch,
            panel,
        });
    }
    if let Some(index) = sample
        .frame_gaps_ms
        .iter()
        .position(|gap| !gap.is_finite() || *gap < 0.0)
    {
        return Err(DesktopScrollComparisonError::InvalidFrameGap {
            report: report_role,
            launch: launch.launch,
            panel,
            index,
        });
    }
    let slow_frame_count = sample
        .frame_gaps_ms
        .iter()
        .filter(|gap| **gap > 33.0)
        .count();
    if sample.frames_exceeding_33_ms != slow_frame_count {
        return Err(DesktopScrollComparisonError::SlowFrameCountMismatch {
            report: report_role,
            launch: launch.launch,
            panel,
        });
    }
    Ok(())
}

fn panel_metrics(report: &DesktopScrollReport, panel: DesktopScrollPanel) -> PanelMetrics {
    let mut frame_gaps_ms = report
        .launches
        .iter()
        .flat_map(|launch| panel.sample(launch).frame_gaps_ms.iter().copied())
        .collect::<Vec<_>>();
    frame_gaps_ms.sort_by(f64::total_cmp);
    PanelMetrics {
        frame_gap_ms: FrameGapMetrics {
            p50: nearest_rank(&frame_gaps_ms, 50),
            p95: nearest_rank(&frame_gaps_ms, 95),
            p99: nearest_rank(&frame_gaps_ms, 99),
        },
        frames_exceeding_33_ms: frame_gaps_ms
            .iter()
            .fold(0_u64, |count, gap| count + u64::from(*gap > 33.0)),
    }
}

fn nearest_rank(sorted_values: &[f64], percentile: usize) -> f64 {
    let rank = sorted_values.len().saturating_mul(percentile).div_ceil(100);
    sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)]
}

fn compare_panels(baseline: PanelMetrics, current: PanelMetrics) -> PanelComparison {
    PanelComparison {
        frame_gap_ms: FrameGapComparison {
            p50: MetricDelta::between(baseline.frame_gap_ms.p50, current.frame_gap_ms.p50),
            p95: MetricDelta::between(baseline.frame_gap_ms.p95, current.frame_gap_ms.p95),
            p99: MetricDelta::between(baseline.frame_gap_ms.p99, current.frame_gap_ms.p99),
        },
        frames_exceeding_33_ms: MetricDelta::between(
            u64_to_f64(baseline.frames_exceeding_33_ms),
            u64_to_f64(current.frames_exceeding_33_ms),
        ),
    }
}

fn u64_to_f64(value: u64) -> f64 {
    let [low_0, low_1, low_2, low_3, high_0, high_1, high_2, high_3] = value.to_le_bytes();
    let low = u32::from_le_bytes([low_0, low_1, low_2, low_3]);
    let high = u32::from_le_bytes([high_0, high_1, high_2, high_3]);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

fn median_u64_as_f64(values: impl Iterator<Item = u64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        f64::midpoint(u64_to_f64(values[middle - 1]), u64_to_f64(values[middle]))
    } else {
        u64_to_f64(values[middle])
    }
}

fn process_cpu_time_milliseconds(report: &DesktopScrollReport) -> f64 {
    median_u64_as_f64(
        report
            .launches
            .iter()
            .map(|launch| launch.readiness.process_cpu_clock_ticks),
    ) / u64_to_f64(report.protocol.process_cpu_clock_ticks_per_second)
        * 1_000.0
}

fn peak_rss_bytes(report: &DesktopScrollReport) -> u64 {
    report
        .launches
        .iter()
        .flat_map(|launch| {
            [
                launch.readiness.peak_memory.rss_bytes,
                launch.memory_after_changed_files.rss_bytes,
                launch.memory_after_commits.rss_bytes,
            ]
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_scroll::{
        DesktopScrollBenchmarkProtocol, DesktopScrollLaunch, DesktopScrollProcessMemory,
        DesktopScrollReadinessSample, DesktopScrollResourceBounds, DesktopScrollRunner,
        DesktopScrollSource, DesktopScrollSystemConditions, DesktopScrollWindow, ScrollProtocol,
    };

    #[test]
    fn comparison_combines_launch_distributions_and_peak_memory() {
        let baseline = report(
            [vec![10.0, 20.0], vec![30.0, 40.0], vec![25.0, 35.0]],
            [vec![16.0, 16.0], vec![16.0, 16.0], vec![16.0, 16.0]],
            [900, 1_000, 1_100],
            [90, 100, 110],
            [100, 120, 110],
        );
        let current = report(
            [vec![20.0, 30.0], vec![40.0, 50.0], vec![35.0, 45.0]],
            [vec![17.0, 17.0], vec![17.0, 17.0], vec![17.0, 17.0]],
            [700, 800, 900],
            [70, 80, 90],
            [140, 150, 145],
        );

        let comparison = compare_reports(&baseline, &current).unwrap();

        assert_close(
            comparison.readiness_wall_time_milliseconds.baseline,
            1_000.0,
        );
        assert_close(comparison.readiness_wall_time_milliseconds.current, 800.0);
        assert_close(
            comparison.readiness_process_cpu_time_milliseconds.baseline,
            1_000.0,
        );
        assert_close(
            comparison.readiness_process_cpu_time_milliseconds.current,
            800.0,
        );
        assert_eq!(
            comparison.changed_files.frame_gap_ms.p50,
            MetricDelta {
                baseline: 25.0,
                current: 35.0,
                relative_change_percent: Some(40.0),
            }
        );
        assert_close(comparison.changed_files.frame_gap_ms.p95.baseline, 40.0);
        assert_close(comparison.changed_files.frame_gap_ms.p95.current, 50.0);
        assert_close(
            comparison.changed_files.frames_exceeding_33_ms.baseline,
            2.0,
        );
        assert_close(comparison.changed_files.frames_exceeding_33_ms.current, 4.0);
        assert_close(comparison.commits.frame_gap_ms.p50.baseline, 16.0);
        assert_close(comparison.commits.frame_gap_ms.p50.current, 17.0);
        assert_close(comparison.peak_rss_bytes.baseline, 120.0);
        assert_close(comparison.peak_rss_bytes.current, 150.0);
        assert_eq!(
            comparison.peak_rss_bytes.relative_change_percent,
            Some(25.0)
        );
    }

    #[test]
    fn zero_baselines_have_no_relative_percentage() {
        let baseline = report(
            [vec![0.0], vec![0.0], vec![0.0]],
            [vec![0.0], vec![0.0], vec![0.0]],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
        );
        let current = report(
            [vec![1.0], vec![1.0], vec![1.0]],
            [vec![1.0], vec![1.0], vec![1.0]],
            [1, 1, 1],
            [1, 1, 1],
            [1, 1, 1],
        );

        let comparison = compare_reports(&baseline, &current).unwrap();

        assert_eq!(
            comparison
                .changed_files
                .frame_gap_ms
                .p50
                .relative_change_percent,
            None
        );
        assert_eq!(comparison.peak_rss_bytes.relative_change_percent, None);
        assert_eq!(
            comparison
                .readiness_process_cpu_time_milliseconds
                .relative_change_percent,
            None
        );
    }

    #[test]
    fn incompatible_protocols_are_rejected() {
        let baseline = standard_report();
        let mut current = standard_report();
        current.protocol.scroll.step_css_pixels += 1;
        for launch in &mut current.launches {
            launch.changed_files.step_css_pixels += 1;
            launch.commits.step_css_pixels += 1;
        }

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(DesktopScrollComparisonError::IncompatibleProtocol)
        );
    }

    #[test]
    fn incompatible_runner_environments_are_rejected() {
        let baseline = standard_report();
        let mut current = standard_report();
        current.runner.cpu_model = "Another CPU".to_owned();

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(DesktopScrollComparisonError::IncompatibleRunnerEnvironment)
        );
    }

    #[test]
    fn missing_raw_frame_gaps_are_rejected() {
        let baseline = standard_report();
        let mut current = standard_report();
        current.launches[0].commits.frame_gaps_ms.clear();
        current.launches[0].commits.frame_gap_count = 0;
        current.launches[0].commits.frame_count = 1;

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(DesktopScrollComparisonError::MissingFrameGaps {
                report: DesktopScrollReportRole::Current,
                launch: 1,
                panel: DesktopScrollPanel::Commits,
            })
        );
    }

    #[test]
    fn mismatched_memory_attribution_is_rejected() {
        let baseline = standard_report();
        let mut current = standard_report();
        current.launches[0].memory_after_commits.attribution = "viewer process tree".to_owned();

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(DesktopScrollComparisonError::MemoryAttributionMismatch {
                report: DesktopScrollReportRole::Current,
                launch: 1,
            })
        );
    }

    #[test]
    fn zero_process_cpu_clock_frequency_is_rejected() {
        let baseline = standard_report();
        let mut current = standard_report();
        current.protocol.process_cpu_clock_ticks_per_second = 0;

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(
                DesktopScrollComparisonError::InvalidProcessCpuClockFrequency {
                    report: DesktopScrollReportRole::Current,
                }
            )
        );
    }

    fn standard_report() -> DesktopScrollReport {
        report(
            [vec![16.0], vec![16.0], vec![16.0]],
            [vec![16.0], vec![16.0], vec![16.0]],
            [1_000, 1_000, 1_000],
            [100, 100, 100],
            [100, 100, 100],
        )
    }

    fn report(
        changed_files_gaps: [Vec<f64>; 3],
        commit_gaps: [Vec<f64>; 3],
        readiness_wall_time_milliseconds: [u64; 3],
        readiness_process_cpu_clock_ticks: [u64; 3],
        peak_rss_bytes: [u64; 3],
    ) -> DesktopScrollReport {
        let launches = changed_files_gaps
            .into_iter()
            .zip(commit_gaps)
            .zip(peak_rss_bytes)
            .enumerate()
            .map(
                |(index, ((changed_files, commits), peak_rss_bytes))| DesktopScrollLaunch {
                    launch: index + 1,
                    conditions_before_launch: conditions(),
                    outer_window: DesktopScrollWindow {
                        x: 20,
                        y: 20,
                        width: 1_200,
                        height: 700,
                    },
                    readiness: DesktopScrollReadinessSample {
                        wall_time_milliseconds: readiness_wall_time_milliseconds[index],
                        process_cpu_clock_ticks: readiness_process_cpu_clock_ticks[index],
                        peak_memory: memory(peak_rss_bytes),
                    },
                    changed_files: sample("changed-files", changed_files),
                    memory_after_changed_files: memory(peak_rss_bytes.saturating_sub(2)),
                    commits: sample("commits", commits),
                    memory_after_commits: memory(peak_rss_bytes.saturating_sub(1)),
                },
            )
            .collect();
        DesktopScrollReport {
            format_version: REPORT_FORMAT_VERSION,
            benchmark: BENCHMARK_NAME.to_owned(),
            source: DesktopScrollSource {
                commit: "0123456789012345678901234567890123456789".to_owned(),
                invocation: "just bench-scroll".to_owned(),
                profile: "release".to_owned(),
            },
            fixture_manifest: toml::from_str(include_str!(
                "../../fixtures/desktop-scroll/manifest.toml"
            ))
            .unwrap(),
            protocol: DesktopScrollBenchmarkProtocol {
                independent_launches: 3,
                outer_window_width_pixels: 1_200,
                outer_window_height_pixels: 700,
                expected_layout: "unified".to_owned(),
                expected_density: "compact".to_owned(),
                readiness: "complete production view".to_owned(),
                memory_attribution: "server and viewer process trees".to_owned(),
                process_cpu_clock_ticks_per_second: 100,
                script_timeout_seconds: 30,
                scroll: ScrollProtocol {
                    distance_css_pixels: 160,
                    step_css_pixels: 8,
                    traversals: 10,
                },
            },
            resource_bounds: DesktopScrollResourceBounds {
                cpu_quota_percent: 200,
                memory_max_bytes: 4 * 1024 * 1024 * 1024,
                memory_swap_max_bytes: 0,
                tasks_max: 512,
                process_niceness: 10,
                cargo_jobs_max: 1,
                rayon_threads_max: 2,
                wall_time_minutes: 20,
                termination_grace_seconds: 15,
            },
            runner: DesktopScrollRunner {
                operating_system: "Example Linux".to_owned(),
                kernel_release: "1.0.0".to_owned(),
                architecture: "x86_64".to_owned(),
                cpu_model: "Example CPU".to_owned(),
                logical_cpu_count: 8,
                rustc_version: "rustc 1.98.0".to_owned(),
                cargo_version: "cargo 1.98.0".to_owned(),
                git_version: "git version 2.0.0".to_owned(),
                tauri_driver_version: "tauri-driver 2.0.0".to_owned(),
                webkitgtk_version: "2.0.0".to_owned(),
            },
            launches,
        }
    }

    fn conditions() -> DesktopScrollSystemConditions {
        DesktopScrollSystemConditions {
            recorded_at_unix_milliseconds: 1,
            load_average_1_minute: 0.1,
            load_average_5_minutes: 0.1,
            load_average_15_minutes: 0.1,
            memory_available_bytes: 1_000,
            cpu_governors: vec!["performance".to_owned()],
            energy_performance_preferences: vec!["performance".to_owned()],
            external_power_online: Some(true),
        }
    }

    fn memory(rss_bytes: u64) -> DesktopScrollProcessMemory {
        DesktopScrollProcessMemory {
            attribution: "server and viewer process trees".to_owned(),
            process_count: 1,
            rss_bytes,
        }
    }

    fn sample(panel: &str, frame_gaps_ms: Vec<f64>) -> ScrollSample {
        let frame_gap_count = frame_gaps_ms.len();
        let frames_exceeding_33_ms = frame_gaps_ms.iter().filter(|gap| **gap > 33.0).count();
        ScrollSample {
            panel: panel.to_owned(),
            distance_css_pixels: 160,
            step_css_pixels: 8,
            traversals: 10,
            total_distance_css_pixels: 1_600,
            scroll_height_css_pixels: 800,
            client_height_css_pixels: 500,
            final_scroll_top_css_pixels: 0.0,
            inner_width_css_pixels: 1_200,
            inner_height_css_pixels: 700,
            frame_count: frame_gap_count + 1,
            frame_gap_count,
            total_duration_ms: frame_gaps_ms.iter().sum(),
            mean_frame_gap_ms: 0.0,
            p50_frame_gap_ms: 0.0,
            p95_frame_gap_ms: 0.0,
            p99_frame_gap_ms: 0.0,
            maximum_frame_gap_ms: 0.0,
            frames_exceeding_33_ms,
            frame_gaps_ms,
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < f64::EPSILON);
    }
}
