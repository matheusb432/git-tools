use super::{
    BENCHMARK_NAME, GrpcTransportReport, REPORT_FORMAT_VERSION, TransportKind, TransportLaunch,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricDelta {
    pub baseline: f64,
    pub current: f64,
    pub relative_change_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchMetricDelta {
    pub baseline_median: f64,
    pub baseline_minimum: f64,
    pub baseline_maximum: f64,
    pub current_median: f64,
    pub current_minimum: f64,
    pub current_maximum: f64,
    pub relative_change_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchMetricSummary {
    pub median: f64,
    pub minimum: f64,
    pub maximum: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrpcTransportSummary {
    pub transport: TransportKind,
    pub latency_p50_nanoseconds: f64,
    pub latency_p95_nanoseconds: f64,
    pub latency_p99_nanoseconds: f64,
    pub requests_per_second: LaunchMetricSummary,
    pub server_cpu_time_per_request_nanoseconds: LaunchMetricSummary,
    pub peak_server_and_client_rss_bytes: LaunchMetricSummary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrpcTransportComparison {
    pub baseline_transport: TransportKind,
    pub current_transport: TransportKind,
    pub latency_p50_nanoseconds: MetricDelta,
    pub latency_p95_nanoseconds: MetricDelta,
    pub latency_p99_nanoseconds: MetricDelta,
    pub requests_per_second: LaunchMetricDelta,
    pub server_cpu_time_per_request_nanoseconds: LaunchMetricDelta,
    pub peak_server_and_client_rss_bytes: LaunchMetricDelta,
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
pub enum GrpcTransportComparisonError {
    #[error("{report} gRPC transport report uses format version {actual}, expected {expected}")]
    UnsupportedFormat {
        report: ReportRole,
        actual: u32,
        expected: u32,
    },
    #[error("{report} gRPC transport report has benchmark identity {actual}")]
    UnexpectedBenchmark { report: ReportRole, actual: String },
    #[error("{report} gRPC transport report has an invalid protocol: {reason}")]
    InvalidProtocol { report: ReportRole, reason: String },
    #[error("{report} gRPC transport report has an invalid launch sequence")]
    InvalidLaunchSequence { report: ReportRole },
    #[error("{report} gRPC transport launch {launch} has invalid ghz evidence: {reason}")]
    InvalidGhzEvidence {
        report: ReportRole,
        launch: usize,
        reason: String,
    },
    #[error("gRPC transport reports use different workload protocols")]
    IncompatibleProtocol,
    #[error("gRPC transport reports use different resource bounds")]
    IncompatibleResourceBounds,
    #[error("gRPC transport reports use different runner environments")]
    IncompatibleRunnerEnvironment,
    #[error("gRPC transport reports use different build profiles")]
    IncompatibleBuildProfile,
}

pub fn validate_report(report: &GrpcTransportReport) -> Result<(), GrpcTransportComparisonError> {
    validate_report_as(ReportRole::Current, report)
}

pub fn summarize_report(
    report: &GrpcTransportReport,
) -> Result<GrpcTransportSummary, GrpcTransportComparisonError> {
    validate_report(report)?;
    Ok(summarize_validated_report(report))
}

pub fn compare_reports(
    baseline: &GrpcTransportReport,
    current: &GrpcTransportReport,
) -> Result<GrpcTransportComparison, GrpcTransportComparisonError> {
    validate_report_as(ReportRole::Baseline, baseline)?;
    validate_report_as(ReportRole::Current, current)?;
    if baseline.protocol != current.protocol {
        return Err(GrpcTransportComparisonError::IncompatibleProtocol);
    }
    if baseline.resource_bounds != current.resource_bounds {
        return Err(GrpcTransportComparisonError::IncompatibleResourceBounds);
    }
    if baseline.runner != current.runner {
        return Err(GrpcTransportComparisonError::IncompatibleRunnerEnvironment);
    }
    if baseline.source.profile != current.source.profile {
        return Err(GrpcTransportComparisonError::IncompatibleBuildProfile);
    }
    let baseline_summary = summarize_validated_report(baseline);
    let current_summary = summarize_validated_report(current);

    Ok(GrpcTransportComparison {
        baseline_transport: baseline.transport,
        current_transport: current.transport,
        latency_p50_nanoseconds: MetricDelta::between(
            baseline_summary.latency_p50_nanoseconds,
            current_summary.latency_p50_nanoseconds,
        ),
        latency_p95_nanoseconds: MetricDelta::between(
            baseline_summary.latency_p95_nanoseconds,
            current_summary.latency_p95_nanoseconds,
        ),
        latency_p99_nanoseconds: MetricDelta::between(
            baseline_summary.latency_p99_nanoseconds,
            current_summary.latency_p99_nanoseconds,
        ),
        requests_per_second: LaunchMetricDelta::between(
            baseline_summary.requests_per_second,
            current_summary.requests_per_second,
        ),
        server_cpu_time_per_request_nanoseconds: LaunchMetricDelta::between(
            baseline_summary.server_cpu_time_per_request_nanoseconds,
            current_summary.server_cpu_time_per_request_nanoseconds,
        ),
        peak_server_and_client_rss_bytes: LaunchMetricDelta::between(
            baseline_summary.peak_server_and_client_rss_bytes,
            current_summary.peak_server_and_client_rss_bytes,
        ),
    })
}

fn summarize_validated_report(report: &GrpcTransportReport) -> GrpcTransportSummary {
    GrpcTransportSummary {
        transport: report.transport,
        latency_p50_nanoseconds: u64_to_f64(latency_percentile(report, 50)),
        latency_p95_nanoseconds: u64_to_f64(latency_percentile(report, 95)),
        latency_p99_nanoseconds: u64_to_f64(latency_percentile(report, 99)),
        requests_per_second: LaunchMetricSummary::from_values(
            report.launches.iter().map(|launch| launch.ghz.rps),
        ),
        server_cpu_time_per_request_nanoseconds: LaunchMetricSummary::from_values(
            report
                .launches
                .iter()
                .map(|launch| server_cpu_per_request(launch, report.protocol.total_request_count)),
        ),
        peak_server_and_client_rss_bytes: LaunchMetricSummary::from_values(
            report
                .launches
                .iter()
                .map(|launch| u64_to_f64(launch.peak_server_and_client_rss_bytes)),
        ),
    }
}

fn validate_report_as(
    report_role: ReportRole,
    report: &GrpcTransportReport,
) -> Result<(), GrpcTransportComparisonError> {
    if report.format_version != REPORT_FORMAT_VERSION {
        return Err(GrpcTransportComparisonError::UnsupportedFormat {
            report: report_role,
            actual: report.format_version,
            expected: REPORT_FORMAT_VERSION,
        });
    }
    if report.benchmark != BENCHMARK_NAME {
        return Err(GrpcTransportComparisonError::UnexpectedBenchmark {
            report: report_role,
            actual: report.benchmark.clone(),
        });
    }
    let protocol = &report.protocol;
    if protocol.independent_launches == 0
        || protocol.rpc.is_empty()
        || protocol.proto_sha256.len() != 64
        || protocol.ghz_config_sha256.len() != 64
        || protocol.total_request_count == 0
        || protocol.warmup_request_count == 0
        || protocol.measured_request_count == 0
        || protocol
            .total_request_count
            .checked_sub(protocol.warmup_request_count)
            != Some(protocol.measured_request_count)
        || protocol.concurrency != 1
        || protocol.connections != 1
        || protocol.client_cpu_count == 0
        || protocol.request_timeout_nanoseconds == 0
        || protocol.ghz_wall_timeout_seconds == 0
        || protocol.rss_sample_interval_microseconds == 0
    {
        return Err(GrpcTransportComparisonError::InvalidProtocol {
            report: report_role,
            reason: "fixed launch, request, connection, timeout, or workload identity drifted"
                .to_owned(),
        });
    }
    if report.launches.len() != protocol.independent_launches
        || report
            .launches
            .iter()
            .enumerate()
            .any(|(index, launch)| launch.launch != index + 1)
    {
        return Err(GrpcTransportComparisonError::InvalidLaunchSequence {
            report: report_role,
        });
    }
    for launch in &report.launches {
        validate_launch(report_role, protocol.measured_request_count, launch)?;
    }
    Ok(())
}

fn validate_launch(
    report_role: ReportRole,
    measured_request_count: u64,
    launch: &TransportLaunch,
) -> Result<(), GrpcTransportComparisonError> {
    let invalid = |reason: &str| GrpcTransportComparisonError::InvalidGhzEvidence {
        report: report_role,
        launch: launch.launch,
        reason: reason.to_owned(),
    };
    let detail_count =
        u64::try_from(launch.ghz.details.len()).map_err(|_| invalid("sample count exceeds u64"))?;
    if !launch.push_confirmation_required {
        return Err(invalid("semantic response check failed"));
    }
    if launch.server_cpu_time_nanoseconds == 0 || launch.peak_server_and_client_rss_bytes == 0 {
        return Err(invalid("process CPU or memory evidence is empty"));
    }
    if launch.ghz.count != measured_request_count || detail_count != measured_request_count {
        return Err(invalid(&format!(
            "expected {measured_request_count} measured requests, ghz reported {} with {detail_count} retained raw samples",
            launch.ghz.count
        )));
    }
    if launch.ghz.total_nanoseconds == 0
        || launch.ghz.average_nanoseconds == 0
        || launch.ghz.fastest_nanoseconds == 0
        || launch.ghz.slowest_nanoseconds == 0
        || !launch.ghz.rps.is_finite()
        || launch.ghz.rps <= 0.0
    {
        return Err(invalid("summary latency or throughput is empty"));
    }
    if !launch.ghz.error_distribution.is_empty()
        || launch.ghz.status_code_distribution.len() != 1
        || launch.ghz.status_code_distribution.get("OK") != Some(&measured_request_count)
    {
        return Err(invalid("requests did not all complete with OK status"));
    }
    if launch.ghz.details.iter().any(|sample| {
        sample.latency_nanoseconds == 0 || !sample.error.is_empty() || sample.status != "OK"
    }) {
        return Err(invalid("a raw request sample is unsuccessful"));
    }
    Ok(())
}

fn latency_percentile(report: &GrpcTransportReport, percentile: usize) -> u64 {
    let mut samples = report
        .launches
        .iter()
        .flat_map(|launch| &launch.ghz.details)
        .map(|sample| sample.latency_nanoseconds)
        .collect::<Vec<_>>();
    samples.sort_unstable();
    let rank = percentile
        .saturating_mul(samples.len())
        .div_ceil(100)
        .saturating_sub(1);
    samples[rank]
}

fn server_cpu_per_request(launch: &TransportLaunch, issued_request_count: u64) -> f64 {
    u64_to_f64(launch.server_cpu_time_nanoseconds) / u64_to_f64(issued_request_count)
}

impl MetricDelta {
    fn between(baseline: f64, current: f64) -> Self {
        Self {
            baseline,
            current,
            relative_change_percent: relative_change(baseline, current),
        }
    }
}

impl LaunchMetricDelta {
    fn between(baseline: LaunchMetricSummary, current: LaunchMetricSummary) -> Self {
        Self {
            baseline_median: baseline.median,
            baseline_minimum: baseline.minimum,
            baseline_maximum: baseline.maximum,
            current_median: current.median,
            current_minimum: current.minimum,
            current_maximum: current.maximum,
            relative_change_percent: relative_change(baseline.median, current.median),
        }
    }
}

impl LaunchMetricSummary {
    fn from_values(values: impl Iterator<Item = f64>) -> Self {
        let mut values = values.collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        let median = if values.len().is_multiple_of(2) {
            f64::midpoint(values[values.len() / 2 - 1], values[values.len() / 2])
        } else {
            values[values.len() / 2]
        };
        Self {
            median,
            minimum: values[0],
            maximum: values[values.len() - 1],
        }
    }
}

fn relative_change(baseline: f64, current: f64) -> Option<f64> {
    (baseline != 0.0).then(|| (current - baseline) / baseline * 100.0)
}

fn u64_to_f64(value: u64) -> f64 {
    let [low_0, low_1, low_2, low_3, high_0, high_1, high_2, high_3] = value.to_le_bytes();
    let low = u32::from_le_bytes([low_0, low_1, low_2, low_3]);
    let high = u32::from_le_bytes([high_0, high_1, high_2, high_3]);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::grpc_transport::{
        BenchmarkSource, GhzMeasurement, GhzSample, GrpcTransportProtocol,
        GrpcTransportResourceBounds, GrpcTransportRunner, TransportLaunch,
    };

    #[test]
    fn comparison_aggregates_raw_samples_and_allows_transport_changes() {
        let baseline = report(TransportKind::Tcp, 100);
        let current = report(TransportKind::Unix, 80);

        let comparison = compare_reports(&baseline, &current).unwrap();

        assert_eq!(comparison.baseline_transport, TransportKind::Tcp);
        assert_eq!(comparison.current_transport, TransportKind::Unix);
        assert_approximately(comparison.latency_p50_nanoseconds.baseline, 102.0);
        assert_approximately(comparison.latency_p50_nanoseconds.current, 82.0);
        assert_approximately(comparison.latency_p95_nanoseconds.baseline, 104.0);
        assert_approximately(comparison.latency_p95_nanoseconds.current, 84.0);
        assert_approximately(comparison.latency_p99_nanoseconds.baseline, 104.0);
        assert_approximately(comparison.latency_p99_nanoseconds.current, 84.0);
        let summary = summarize_report(&current).unwrap();
        assert_eq!(summary.transport, TransportKind::Unix);
        assert_approximately(summary.requests_per_second.median, 10_002.0);
    }

    #[test]
    fn comparison_rejects_workload_drift() {
        let baseline = report(TransportKind::Tcp, 100);
        let mut current = report(TransportKind::Unix, 80);
        current.protocol.connections = 2;

        assert_eq!(
            compare_reports(&baseline, &current),
            Err(GrpcTransportComparisonError::InvalidProtocol {
                report: ReportRole::Current,
                reason: "fixed launch, request, connection, timeout, or workload identity drifted"
                    .to_owned(),
            })
        );
    }

    #[test]
    fn validation_rejects_unsuccessful_raw_requests() {
        let mut current = report(TransportKind::Tcp, 100);
        current.launches[1].ghz.details[0].status = "Unavailable".to_owned();

        assert!(matches!(
            validate_report(&current),
            Err(GrpcTransportComparisonError::InvalidGhzEvidence { launch: 2, .. })
        ));
    }

    fn report(transport: TransportKind, latency_start: u64) -> GrpcTransportReport {
        let launches = (1..=3)
            .map(|launch| TransportLaunch {
                launch,
                server_cpu_time_nanoseconds: 300,
                peak_server_and_client_rss_bytes: 1_024 * launch as u64,
                push_confirmation_required: true,
                ghz: GhzMeasurement {
                    count: 3,
                    total_nanoseconds: 300,
                    average_nanoseconds: 100,
                    fastest_nanoseconds: latency_start,
                    slowest_nanoseconds: latency_start + 4,
                    rps: [10_001.0, 10_002.0, 10_003.0][launch - 1],
                    error_distribution: BTreeMap::new(),
                    status_code_distribution: BTreeMap::from([("OK".to_owned(), 3)]),
                    details: (0..3)
                        .map(|offset| GhzSample {
                            timestamp: format!("2026-08-27T00:00:0{offset}Z"),
                            latency_nanoseconds: latency_start + (launch - 1) as u64 + offset,
                            error: String::new(),
                            status: "OK".to_owned(),
                        })
                        .collect(),
                },
            })
            .collect();
        GrpcTransportReport {
            format_version: REPORT_FORMAT_VERSION,
            benchmark: BENCHMARK_NAME.to_owned(),
            source: BenchmarkSource {
                commit: "a".repeat(40),
                invocation: "just bench-grpc".to_owned(),
                profile: "release".to_owned(),
            },
            transport,
            protocol: GrpcTransportProtocol {
                independent_launches: 3,
                rpc: "gtl.v1.SettingsService.GetPushConfirmationRequirement".to_owned(),
                proto_sha256: "b".repeat(64),
                ghz_config_sha256: "c".repeat(64),
                total_request_count: 4,
                warmup_request_count: 1,
                measured_request_count: 3,
                concurrency: 1,
                connections: 1,
                client_cpu_count: 1,
                request_timeout_nanoseconds: 1_000_000_000,
                ghz_wall_timeout_seconds: 600,
                memory_attribution: "gtl-server and ghz processes".to_owned(),
                rss_sample_interval_microseconds: 1_000,
            },
            resource_bounds: GrpcTransportResourceBounds {
                cpu_quota_percent: 200,
                memory_max_bytes: 2 * 1_024 * 1_024 * 1_024,
                memory_swap_max_bytes: 0,
                tasks_max: 128,
                process_niceness: 10,
                cargo_jobs_max: 1,
                wall_time_minutes: 10,
                termination_grace_seconds: 15,
            },
            runner: GrpcTransportRunner {
                operating_system: "Ubuntu".to_owned(),
                kernel_release: "kernel".to_owned(),
                architecture: "x86_64".to_owned(),
                cpu_model: "cpu".to_owned(),
                logical_cpu_count: 16,
                rustc_version: "rustc".to_owned(),
                ghz_version: "v0.121.0".to_owned(),
            },
            launches,
        }
    }

    fn assert_approximately(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < f64::EPSILON);
    }
}
