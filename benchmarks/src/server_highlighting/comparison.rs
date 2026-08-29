use std::collections::BTreeMap;

use super::{
    BENCHMARK_NAME, HighlightWorkload, REPORT_FORMAT_VERSION, ServerHighlightingReport,
    StreamEvidence, StreamMeasurement, StreamTemperature,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricDelta {
    pub baseline_median: f64,
    pub baseline_minimum: f64,
    pub baseline_maximum: f64,
    pub current_median: f64,
    pub current_minimum: f64,
    pub current_maximum: f64,
    pub relative_change_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleComparison {
    pub peak_rss_bytes: MetricDelta,
    pub wall_time_microseconds: MetricDelta,
    pub server_cpu_time_microseconds: MetricDelta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkloadComparison {
    pub workload: HighlightWorkload,
    pub cold: SampleComparison,
    pub warm: SampleComparison,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerHighlightingComparison {
    pub workloads: Vec<WorkloadComparison>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportRole {
    Baseline,
    Current,
}

impl std::fmt::Display for ReportRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Baseline => formatter.write_str("baseline"),
            Self::Current => formatter.write_str("current"),
        }
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ComparisonError {
    #[error(
        "{report} server highlighting report uses format version {actual}, expected {expected}"
    )]
    UnsupportedFormat {
        report: ReportRole,
        actual: u32,
        expected: u32,
    },
    #[error("{report} server highlighting report has benchmark identity {actual}")]
    UnexpectedBenchmark { report: ReportRole, actual: String },
    #[error("{report} server highlighting report has an invalid protocol: {reason}")]
    InvalidProtocol { report: ReportRole, reason: String },
    #[error("{report} server highlighting report has an invalid fixture set")]
    InvalidFixtures { report: ReportRole },
    #[error("{report} server highlighting report has an invalid launch sequence")]
    InvalidLaunchSequence { report: ReportRole },
    #[error("{report} server highlighting report has an invalid residency sequence")]
    InvalidResidencySequence { report: ReportRole },
    #[error("{report} server highlighting report has inconsistent semantic evidence")]
    InconsistentSemanticEvidence { report: ReportRole },
    #[error("server highlighting reports use different fixtures")]
    IncompatibleFixtures,
    #[error("server highlighting reports use different protocols")]
    IncompatibleProtocol,
    #[error("server highlighting reports use different resource bounds")]
    IncompatibleResourceBounds,
    #[error("server highlighting reports use different runner environments")]
    IncompatibleRunnerEnvironment,
    #[error("server highlighting reports use different build profiles")]
    IncompatibleBuildProfile,
    #[error("server highlighting reports produced different semantic evidence")]
    SemanticMismatch,
}

pub fn validate_report(report: &ServerHighlightingReport) -> Result<(), ComparisonError> {
    validate_report_as(ReportRole::Current, report)
}

pub fn compare_reports(
    baseline: &ServerHighlightingReport,
    current: &ServerHighlightingReport,
) -> Result<ServerHighlightingComparison, ComparisonError> {
    validate_report_as(ReportRole::Baseline, baseline)?;
    validate_report_as(ReportRole::Current, current)?;
    if baseline.fixtures != current.fixtures {
        return Err(ComparisonError::IncompatibleFixtures);
    }
    if baseline.protocol != current.protocol {
        return Err(ComparisonError::IncompatibleProtocol);
    }
    if baseline.resource_bounds != current.resource_bounds {
        return Err(ComparisonError::IncompatibleResourceBounds);
    }
    if baseline.runner != current.runner {
        return Err(ComparisonError::IncompatibleRunnerEnvironment);
    }
    if baseline.source.profile != current.source.profile {
        return Err(ComparisonError::IncompatibleBuildProfile);
    }
    if semantic_evidence(baseline) != semantic_evidence(current) {
        return Err(ComparisonError::SemanticMismatch);
    }

    let workloads = baseline
        .protocol
        .workloads
        .iter()
        .copied()
        .map(|workload| WorkloadComparison {
            workload,
            cold: compare_samples(baseline, current, workload, StreamTemperature::Cold),
            warm: compare_samples(baseline, current, workload, StreamTemperature::Warm),
        })
        .collect();
    Ok(ServerHighlightingComparison { workloads })
}

fn validate_report_as(
    role: ReportRole,
    report: &ServerHighlightingReport,
) -> Result<(), ComparisonError> {
    if report.format_version != REPORT_FORMAT_VERSION {
        return Err(ComparisonError::UnsupportedFormat {
            report: role,
            actual: report.format_version,
            expected: REPORT_FORMAT_VERSION,
        });
    }
    if report.benchmark != BENCHMARK_NAME {
        return Err(ComparisonError::UnexpectedBenchmark {
            report: role,
            actual: report.benchmark.clone(),
        });
    }
    if report.protocol.independent_launches == 0
        || report.protocol.workloads != HighlightWorkload::ALL
        || report.protocol.temperatures != StreamTemperature::ALL
        || report.protocol.residency_sequence
            != [
                HighlightWorkload::Rust,
                HighlightWorkload::Full,
                HighlightWorkload::Rust,
            ]
        || report.protocol.process_cpu_clock_ticks_per_second == 0
    {
        return Err(ComparisonError::InvalidProtocol {
            report: role,
            reason: "fixed workload, temperature, residency, launch, or CPU-clock contract drifted"
                .to_owned(),
        });
    }
    let fixture_workloads = report
        .fixtures
        .iter()
        .map(|fixture| fixture.workload)
        .collect::<Vec<_>>();
    if fixture_workloads != report.protocol.workloads {
        return Err(ComparisonError::InvalidFixtures { report: role });
    }
    validate_independent_launches(role, report)?;
    validate_residency_launches(role, report)?;
    validate_semantics(role, report)
}

fn validate_independent_launches(
    role: ReportRole,
    report: &ServerHighlightingReport,
) -> Result<(), ComparisonError> {
    let expected_count = report
        .protocol
        .independent_launches
        .saturating_mul(report.protocol.workloads.len());
    if report.launches.len() != expected_count {
        return Err(ComparisonError::InvalidLaunchSequence { report: role });
    }
    for (index, launch) in report.launches.iter().enumerate() {
        let expected_launch = index / report.protocol.workloads.len() + 1;
        let expected_workload = report.protocol.workloads[index % report.protocol.workloads.len()];
        if launch.launch != expected_launch
            || launch.workload != expected_workload
            || launch.samples.len() != report.protocol.temperatures.len()
            || launch
                .samples
                .iter()
                .map(|sample| sample.temperature)
                .ne(report.protocol.temperatures.iter().copied())
        {
            return Err(ComparisonError::InvalidLaunchSequence { report: role });
        }
    }
    Ok(())
}

fn validate_residency_launches(
    role: ReportRole,
    report: &ServerHighlightingReport,
) -> Result<(), ComparisonError> {
    if report.residency_launches.len() != report.protocol.independent_launches {
        return Err(ComparisonError::InvalidResidencySequence { report: role });
    }
    for (index, launch) in report.residency_launches.iter().enumerate() {
        if launch.launch != index + 1
            || launch
                .samples
                .iter()
                .map(|sample| sample.workload)
                .ne(report.protocol.residency_sequence.iter().copied())
        {
            return Err(ComparisonError::InvalidResidencySequence { report: role });
        }
    }
    Ok(())
}

fn validate_semantics(
    role: ReportRole,
    report: &ServerHighlightingReport,
) -> Result<(), ComparisonError> {
    let mut expected = BTreeMap::new();
    let independent_evidence = report.launches.iter().flat_map(|launch| {
        launch
            .samples
            .iter()
            .map(move |sample| (launch.workload, &sample.measurement.evidence))
    });
    for (workload, evidence) in independent_evidence {
        record_semantic_evidence(role, workload, evidence, &mut expected)?;
    }

    let residency_evidence = report
        .residency_launches
        .iter()
        .flat_map(|launch| &launch.samples);
    for sample in residency_evidence {
        ensure_semantic_evidence_matches(
            role,
            sample.workload,
            &sample.measurement.evidence,
            &expected,
        )?;
    }
    Ok(())
}

fn record_semantic_evidence<'report>(
    role: ReportRole,
    workload: HighlightWorkload,
    evidence: &'report StreamEvidence,
    expected: &mut BTreeMap<HighlightWorkload, &'report StreamEvidence>,
) -> Result<(), ComparisonError> {
    if evidence.message_count == 0
        || evidence.row_count == 0
        || evidence.encoded_bytes == 0
        || evidence.semantic_sha256.len() != 64
    {
        return Err(ComparisonError::InconsistentSemanticEvidence { report: role });
    }
    if expected
        .insert(workload, evidence)
        .is_some_and(|previous| previous != evidence)
    {
        return Err(ComparisonError::InconsistentSemanticEvidence { report: role });
    }
    Ok(())
}

fn ensure_semantic_evidence_matches(
    role: ReportRole,
    workload: HighlightWorkload,
    evidence: &StreamEvidence,
    expected: &BTreeMap<HighlightWorkload, &StreamEvidence>,
) -> Result<(), ComparisonError> {
    if expected.get(&workload) != Some(&evidence) {
        return Err(ComparisonError::InconsistentSemanticEvidence { report: role });
    }
    Ok(())
}

fn semantic_evidence(
    report: &ServerHighlightingReport,
) -> BTreeMap<HighlightWorkload, &super::StreamEvidence> {
    report
        .launches
        .iter()
        .map(|launch| (launch.workload, &launch.samples[0].measurement.evidence))
        .collect()
}

fn compare_samples(
    baseline: &ServerHighlightingReport,
    current: &ServerHighlightingReport,
    workload: HighlightWorkload,
    temperature: StreamTemperature,
) -> SampleComparison {
    let baseline_samples = measurements(baseline, workload, temperature);
    let current_samples = measurements(current, workload, temperature);
    SampleComparison {
        peak_rss_bytes: MetricDelta::between(
            baseline_samples
                .iter()
                .map(|sample| u64_to_f64(sample.peak_rss_bytes)),
            current_samples
                .iter()
                .map(|sample| u64_to_f64(sample.peak_rss_bytes)),
        ),
        wall_time_microseconds: MetricDelta::between(
            baseline_samples
                .iter()
                .map(|sample| u64_to_f64(sample.wall_time_microseconds)),
            current_samples
                .iter()
                .map(|sample| u64_to_f64(sample.wall_time_microseconds)),
        ),
        server_cpu_time_microseconds: MetricDelta::between(
            baseline_samples
                .iter()
                .map(|sample| u64_to_f64(sample.server_cpu_time_nanoseconds) / 1_000.0),
            current_samples
                .iter()
                .map(|sample| u64_to_f64(sample.server_cpu_time_nanoseconds) / 1_000.0),
        ),
    }
}

fn measurements(
    report: &ServerHighlightingReport,
    workload: HighlightWorkload,
    temperature: StreamTemperature,
) -> Vec<&StreamMeasurement> {
    report
        .launches
        .iter()
        .filter(|launch| launch.workload == workload)
        .flat_map(|launch| &launch.samples)
        .filter(|sample| sample.temperature == temperature)
        .map(|sample| &sample.measurement)
        .collect()
}

impl MetricDelta {
    fn between(baseline: impl Iterator<Item = f64>, current: impl Iterator<Item = f64>) -> Self {
        let (baseline_median, baseline_minimum, baseline_maximum) = summarize(baseline);
        let (current_median, current_minimum, current_maximum) = summarize(current);
        Self {
            baseline_median,
            baseline_minimum,
            baseline_maximum,
            current_median,
            current_minimum,
            current_maximum,
            relative_change_percent: (baseline_median != 0.0)
                .then(|| (current_median - baseline_median) / baseline_median * 100.0),
        }
    }
}

fn summarize(values: impl Iterator<Item = f64>) -> (f64, f64, f64) {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let median = if values.len().is_multiple_of(2) {
        f64::midpoint(values[values.len() / 2 - 1], values[values.len() / 2])
    } else {
        values[values.len() / 2]
    };
    (median, values[0], values[values.len() - 1])
}

fn u64_to_f64(value: u64) -> f64 {
    let [low_0, low_1, low_2, low_3, high_0, high_1, high_2, high_3] = value.to_le_bytes();
    let low = u32::from_le_bytes([low_0, low_1, low_2, low_3]);
    let high = u32::from_le_bytes([high_0, high_1, high_2, high_3]);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_highlighting::{
        BenchmarkSource, FixtureIdentity, HighlightLaunch, HighlightProtocol, HighlightSample,
        ProcessMemory, ResidencyLaunch, ResidencySample, ResourceBounds, RunnerEnvironment,
        ServerHighlightingReport, StreamEvidence, StreamMeasurement, SystemConditions,
    };

    #[test]
    fn comparison_uses_launch_medians_and_preserves_ranges() {
        let baseline = report([100, 300, 200]);
        let current = report([80, 160, 120]);

        let comparison = compare_reports(&baseline, &current).unwrap();
        let rust = comparison
            .workloads
            .iter()
            .find(|comparison| comparison.workload == HighlightWorkload::Rust)
            .unwrap();

        assert_close(rust.cold.peak_rss_bytes.baseline_median, 200.0);
        assert_close(rust.cold.peak_rss_bytes.baseline_minimum, 100.0);
        assert_close(rust.cold.peak_rss_bytes.baseline_maximum, 300.0);
        assert_close(rust.cold.peak_rss_bytes.current_median, 120.0);
        assert_close(
            rust.cold.peak_rss_bytes.relative_change_percent.unwrap(),
            -40.0,
        );
    }

    #[test]
    fn comparison_rejects_semantic_drift() {
        let baseline = report([100, 100, 100]);
        let mut current = report([90, 90, 90]);
        current.launches[0].samples[0]
            .measurement
            .evidence
            .semantic_sha256 = "b".repeat(64);
        for sample in &mut current.launches[0].samples[1..] {
            sample.measurement.evidence.semantic_sha256 = "b".repeat(64);
        }

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(ComparisonError::InconsistentSemanticEvidence {
                report: ReportRole::Current
            })
        );
    }

    #[test]
    fn validation_rejects_out_of_order_launches() {
        let mut current = report([100, 100, 100]);
        current.launches.swap(0, 1);

        assert_eq!(
            validate_report(&current),
            Err(ComparisonError::InvalidLaunchSequence {
                report: ReportRole::Current
            })
        );
    }

    fn report(rust_peak_rss: [u64; 3]) -> ServerHighlightingReport {
        let protocol = HighlightProtocol {
            independent_launches: 3,
            workloads: HighlightWorkload::ALL.to_vec(),
            temperatures: StreamTemperature::ALL.to_vec(),
            residency_sequence: vec![
                HighlightWorkload::Rust,
                HighlightWorkload::Full,
                HighlightWorkload::Rust,
            ],
            layout: "unified".to_owned(),
            density: "compact".to_owned(),
            stream_scope: "all-files".to_owned(),
            rss_attribution: "server-pid".to_owned(),
            rss_sample_interval_microseconds: 1_000,
            process_cpu_clock_ticks_per_second: 100,
        };
        let fixtures = HighlightWorkload::ALL
            .into_iter()
            .map(|workload| FixtureIdentity {
                workload,
                name: workload.to_string(),
                source_sha256: "c".repeat(64),
                file_count: 1,
                changed_line_count: 1,
                changed_bytes: 1,
                last_commit_count: 1,
            })
            .collect();
        let launches = (1..=3)
            .flat_map(|launch| {
                HighlightWorkload::ALL.map(move |workload| HighlightLaunch {
                    launch,
                    workload,
                    conditions_before_launch: conditions(),
                    samples: StreamTemperature::ALL
                        .into_iter()
                        .map(|temperature| HighlightSample {
                            temperature,
                            measurement: measurement(fixture_peak_rss(
                                workload,
                                launch,
                                &rust_peak_rss,
                            )),
                        })
                        .collect(),
                })
            })
            .collect();
        let residency_launches = (1..=3)
            .map(|launch| ResidencyLaunch {
                launch,
                conditions_before_launch: conditions(),
                samples: protocol
                    .residency_sequence
                    .iter()
                    .copied()
                    .map(|workload| ResidencySample {
                        workload,
                        measurement: measurement(fixture_peak_rss(
                            workload,
                            launch,
                            &rust_peak_rss,
                        )),
                    })
                    .collect(),
            })
            .collect();
        ServerHighlightingReport {
            format_version: REPORT_FORMAT_VERSION,
            benchmark: BENCHMARK_NAME.to_owned(),
            source: BenchmarkSource {
                commit: "a".repeat(40),
                invocation: "just bench-highlight".to_owned(),
                profile: "release".to_owned(),
            },
            fixtures,
            protocol,
            resource_bounds: ResourceBounds {
                cpu_quota_percent: 200,
                memory_max_bytes: 1,
                memory_swap_max_bytes: 0,
                tasks_max: 32,
                process_niceness: 10,
                cargo_jobs_max: 1,
                wall_time_minutes: 1,
                termination_grace_seconds: 1,
            },
            runner: RunnerEnvironment {
                operating_system: "linux".to_owned(),
                kernel_release: "test".to_owned(),
                architecture: "x86_64".to_owned(),
                cpu_model: "test".to_owned(),
                logical_cpu_count: 2,
                rustc_version: "rustc test".to_owned(),
                cargo_version: "cargo test".to_owned(),
                git_version: "git test".to_owned(),
            },
            launches,
            residency_launches,
        }
    }

    fn fixture_peak_rss(
        workload: HighlightWorkload,
        launch: usize,
        rust_peak_rss: &[u64; 3],
    ) -> u64 {
        if workload == HighlightWorkload::Rust {
            rust_peak_rss[launch - 1]
        } else {
            50
        }
    }

    fn measurement(peak_rss_bytes: u64) -> StreamMeasurement {
        StreamMeasurement {
            wall_time_microseconds: peak_rss_bytes,
            server_cpu_time_nanoseconds: peak_rss_bytes.saturating_mul(1_000),
            server_cpu_clock_ticks: peak_rss_bytes,
            memory_before: memory(10),
            peak_rss_bytes,
            memory_after: memory(20),
            evidence: StreamEvidence {
                message_count: 1,
                row_count: 1,
                encoded_bytes: 1,
                semantic_sha256: "a".repeat(64),
            },
        }
    }

    const fn memory(rss_bytes: u64) -> ProcessMemory {
        ProcessMemory {
            rss: rss_bytes,
            high_water: rss_bytes,
            anonymous: rss_bytes,
            private_dirty: rss_bytes,
        }
    }

    fn conditions() -> SystemConditions {
        SystemConditions {
            recorded_at_unix_milliseconds: 0,
            load_average_1_minute: 0.0,
            load_average_5_minutes: 0.0,
            load_average_15_minutes: 0.0,
            memory_available_bytes: 1,
            cpu_governors: vec!["performance".to_owned()],
            energy_performance_preferences: vec!["performance".to_owned()],
            external_power_online: Some(true),
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < f64::EPSILON);
    }
}
