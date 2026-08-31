use super::{
    BENCHMARK_NAME, FixtureIdentity, REPORT_FORMAT_VERSION, ViewSourceDescriptor,
    ViewSourceOperation, ViewSourceProtocol, ViewSourceReport, ViewSourceSample,
    ViewSourceWorkload,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricSummary {
    pub median: f64,
    pub minimum: f64,
    pub maximum: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricDelta {
    pub baseline: MetricSummary,
    pub current: MetricSummary,
    pub relative_change_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OperationSummary {
    pub wall_time_microseconds: MetricSummary,
    pub process_tree_cpu_time_microseconds: MetricSummary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurrentWorkloadSummary {
    pub workload: ViewSourceWorkload,
    pub compact: OperationSummary,
    pub eager_full_context: OperationSummary,
    pub first_full_context_transition: OperationSummary,
    pub compact_cache_weight_bytes: u64,
    pub full_context_cache_weight_bytes: u64,
    pub compact_wall_change_from_eager_percent: Option<f64>,
    pub compact_cpu_change_from_eager_percent: Option<f64>,
    pub compact_cache_change_from_eager_percent: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HistoricalOperationComparison {
    pub operation: ViewSourceOperation,
    pub wall_time_microseconds: MetricDelta,
    pub process_tree_cpu_time_microseconds: MetricDelta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoricalWorkloadComparison {
    pub workload: ViewSourceWorkload,
    pub operations: Vec<HistoricalOperationComparison>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewSourceComparison {
    pub workloads: Vec<HistoricalWorkloadComparison>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportRole {
    Baseline,
    Current,
    Descriptor,
}

impl std::fmt::Display for ReportRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Baseline => "baseline",
            Self::Current => "current",
            Self::Descriptor => "descriptor",
        })
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ComparisonError {
    #[error("{report} view-source report uses format version {actual}, expected {expected}")]
    UnsupportedFormat {
        report: ReportRole,
        actual: u32,
        expected: u32,
    },
    #[error("{report} view-source report has benchmark identity {actual}")]
    UnexpectedBenchmark { report: ReportRole, actual: String },
    #[error("{report} view-source report has an invalid protocol")]
    InvalidProtocol { report: ReportRole },
    #[error("{report} view-source report has an invalid fixture set")]
    InvalidFixtures { report: ReportRole },
    #[error("{report} view-source report has an invalid launch sequence")]
    InvalidLaunchSequence { report: ReportRole },
    #[error("{report} view-source report has inconsistent semantic evidence")]
    InconsistentSemanticEvidence { report: ReportRole },
    #[error("view-source reports use different fixtures")]
    IncompatibleFixtures,
    #[error("view-source reports use different protocols")]
    IncompatibleProtocol,
    #[error("view-source reports use different runner environments")]
    IncompatibleRunnerEnvironment,
    #[error("view-source reports use different build profiles")]
    IncompatibleBuildProfile,
}

pub fn validate_descriptor(descriptor: &ViewSourceDescriptor) -> Result<(), ComparisonError> {
    validate_descriptor_as(ReportRole::Descriptor, descriptor)
}

pub fn validate_report(report: &ViewSourceReport) -> Result<(), ComparisonError> {
    validate_report_as(ReportRole::Current, report)
}

pub fn ensure_compatible(
    baseline: &ViewSourceReport,
    descriptor: &ViewSourceDescriptor,
) -> Result<(), ComparisonError> {
    validate_report_as(ReportRole::Baseline, baseline)?;
    validate_descriptor(descriptor)?;
    compare_compatibility(
        &baseline.fixtures,
        &baseline.protocol,
        &baseline.runner,
        &baseline.source.profile,
        descriptor,
    )
}

pub fn compare_reports(
    baseline: &ViewSourceReport,
    current: &ViewSourceReport,
) -> Result<ViewSourceComparison, ComparisonError> {
    validate_report_as(ReportRole::Baseline, baseline)?;
    validate_report_as(ReportRole::Current, current)?;
    let descriptor = ViewSourceDescriptor {
        format_version: current.format_version,
        benchmark: current.benchmark.clone(),
        profile: current.source.profile.clone(),
        fixtures: current.fixtures.clone(),
        protocol: current.protocol.clone(),
        runner: current.runner.clone(),
    };
    compare_compatibility(
        &baseline.fixtures,
        &baseline.protocol,
        &baseline.runner,
        &baseline.source.profile,
        &descriptor,
    )?;

    Ok(ViewSourceComparison {
        workloads: ViewSourceWorkload::ALL
            .into_iter()
            .map(|workload| HistoricalWorkloadComparison {
                workload,
                operations: ViewSourceOperation::ALL
                    .into_iter()
                    .map(|operation| HistoricalOperationComparison {
                        operation,
                        wall_time_microseconds: MetricDelta::between(
                            operation_values(baseline, workload, operation, Metric::Wall),
                            operation_values(current, workload, operation, Metric::Wall),
                        ),
                        process_tree_cpu_time_microseconds: MetricDelta::between(
                            operation_values(baseline, workload, operation, Metric::Cpu),
                            operation_values(current, workload, operation, Metric::Cpu),
                        ),
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[must_use]
pub fn summarize_current(report: &ViewSourceReport) -> Vec<CurrentWorkloadSummary> {
    ViewSourceWorkload::ALL
        .into_iter()
        .map(|workload| {
            let compact =
                operation_summary(report, workload, ViewSourceOperation::CompactConstruction);
            let eager = operation_summary(
                report,
                workload,
                ViewSourceOperation::EagerFullContextConstruction,
            );
            let transition = operation_summary(
                report,
                workload,
                ViewSourceOperation::FirstFullContextTransition,
            );
            let sample = sample(report, workload);
            CurrentWorkloadSummary {
                workload,
                compact,
                eager_full_context: eager,
                first_full_context_transition: transition,
                compact_cache_weight_bytes: sample.compact_cache_weight_bytes,
                full_context_cache_weight_bytes: sample.full_context_cache_weight_bytes,
                compact_wall_change_from_eager_percent: relative_change(
                    eager.wall_time_microseconds.median,
                    compact.wall_time_microseconds.median,
                ),
                compact_cpu_change_from_eager_percent: relative_change(
                    eager.process_tree_cpu_time_microseconds.median,
                    compact.process_tree_cpu_time_microseconds.median,
                ),
                compact_cache_change_from_eager_percent: relative_change(
                    u64_to_f64(sample.full_context_cache_weight_bytes),
                    u64_to_f64(sample.compact_cache_weight_bytes),
                ),
            }
        })
        .collect()
}

impl MetricDelta {
    fn between(baseline: Vec<f64>, current: Vec<f64>) -> Self {
        let baseline = MetricSummary::from_values(baseline);
        let current = MetricSummary::from_values(current);
        Self {
            relative_change_percent: relative_change(baseline.median, current.median),
            baseline,
            current,
        }
    }
}

impl MetricSummary {
    fn from_values(mut values: Vec<f64>) -> Self {
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        let median = if values.len().is_multiple_of(2) {
            f64::midpoint(values[middle - 1], values[middle])
        } else {
            values[middle]
        };
        Self {
            median,
            minimum: values[0],
            maximum: values[values.len() - 1],
        }
    }
}

fn validate_report_as(role: ReportRole, report: &ViewSourceReport) -> Result<(), ComparisonError> {
    let descriptor = ViewSourceDescriptor {
        format_version: report.format_version,
        benchmark: report.benchmark.clone(),
        profile: report.source.profile.clone(),
        fixtures: report.fixtures.clone(),
        protocol: report.protocol.clone(),
        runner: report.runner.clone(),
    };
    validate_descriptor_as(role, &descriptor)?;
    if report.launches.len() != report.protocol.independent_launches {
        return Err(ComparisonError::InvalidLaunchSequence { report: role });
    }
    for (index, launch) in report.launches.iter().enumerate() {
        if launch.launch != index + 1
            || launch.samples.len() != ViewSourceWorkload::ALL.len()
            || launch
                .samples
                .iter()
                .map(|sample| sample.workload)
                .ne(ViewSourceWorkload::ALL)
        {
            return Err(ComparisonError::InvalidLaunchSequence { report: role });
        }
        if launch
            .samples
            .iter()
            .zip(&report.fixtures)
            .any(|(sample, fixture)| !sample_matches_fixture(sample, fixture))
        {
            return Err(ComparisonError::InconsistentSemanticEvidence { report: role });
        }
    }
    let first_samples = &report.launches[0].samples;
    if report.launches.iter().skip(1).any(|launch| {
        launch
            .samples
            .iter()
            .zip(first_samples)
            .any(|(candidate, first)| !samples_have_stable_evidence(candidate, first))
    }) {
        return Err(ComparisonError::InconsistentSemanticEvidence { report: role });
    }
    Ok(())
}

fn sample_matches_fixture(sample: &ViewSourceSample, fixture: &FixtureIdentity) -> bool {
    sample.workload == fixture.workload
        && sample.compact_semantic_sha256 == fixture.compact_semantic_sha256
        && sample.full_semantic_sha256 == fixture.full_semantic_sha256
        && sample.full_context_cache_weight_bytes >= sample.compact_cache_weight_bytes
}

fn samples_have_stable_evidence(candidate: &ViewSourceSample, first: &ViewSourceSample) -> bool {
    candidate.compact_cache_weight_bytes == first.compact_cache_weight_bytes
        && candidate.full_context_cache_weight_bytes == first.full_context_cache_weight_bytes
        && candidate.compact_semantic_sha256 == first.compact_semantic_sha256
        && candidate.full_semantic_sha256 == first.full_semantic_sha256
}

fn validate_descriptor_as(
    role: ReportRole,
    descriptor: &ViewSourceDescriptor,
) -> Result<(), ComparisonError> {
    if descriptor.format_version != REPORT_FORMAT_VERSION {
        return Err(ComparisonError::UnsupportedFormat {
            report: role,
            actual: descriptor.format_version,
            expected: REPORT_FORMAT_VERSION,
        });
    }
    if descriptor.benchmark != BENCHMARK_NAME {
        return Err(ComparisonError::UnexpectedBenchmark {
            report: role,
            actual: descriptor.benchmark.clone(),
        });
    }
    if !valid_protocol(&descriptor.protocol) {
        return Err(ComparisonError::InvalidProtocol { report: role });
    }
    if descriptor
        .fixtures
        .iter()
        .map(|fixture| fixture.workload)
        .ne(ViewSourceWorkload::ALL)
        || descriptor.fixtures.iter().any(|fixture| {
            fixture.modified_file_count == 0
                || fixture.full_context_diff_rows <= fixture.compact_diff_rows
                || fixture.full_context_diff_bytes <= fixture.compact_diff_bytes
        })
    {
        return Err(ComparisonError::InvalidFixtures { report: role });
    }
    Ok(())
}

fn valid_protocol(protocol: &ViewSourceProtocol) -> bool {
    protocol.independent_launches == 3
        && protocol.workloads == ViewSourceWorkload::ALL
        && protocol.case_order_by_launch
            == [
                super::CaseOrder {
                    launch: 1,
                    operations: ViewSourceOperation::ALL.to_vec(),
                },
                super::CaseOrder {
                    launch: 2,
                    operations: vec![
                        ViewSourceOperation::EagerFullContextConstruction,
                        ViewSourceOperation::FirstFullContextTransition,
                        ViewSourceOperation::CompactConstruction,
                    ],
                },
                super::CaseOrder {
                    launch: 3,
                    operations: vec![
                        ViewSourceOperation::FirstFullContextTransition,
                        ViewSourceOperation::CompactConstruction,
                        ViewSourceOperation::EagerFullContextConstruction,
                    ],
                },
            ]
        && protocol.sparse_source_line_count == 20_000
        && protocol.sparse_change_stride == 200
        && !protocol.process_tree_cpu_accounting.is_empty()
        && !protocol.measurement_scope.is_empty()
}

fn compare_compatibility(
    fixtures: &[FixtureIdentity],
    protocol: &ViewSourceProtocol,
    runner: &super::RunnerEnvironment,
    profile: &str,
    descriptor: &ViewSourceDescriptor,
) -> Result<(), ComparisonError> {
    if fixtures != descriptor.fixtures {
        return Err(ComparisonError::IncompatibleFixtures);
    }
    if protocol != &descriptor.protocol {
        return Err(ComparisonError::IncompatibleProtocol);
    }
    if runner != &descriptor.runner {
        return Err(ComparisonError::IncompatibleRunnerEnvironment);
    }
    if profile != descriptor.profile {
        return Err(ComparisonError::IncompatibleBuildProfile);
    }
    Ok(())
}

fn sample(report: &ViewSourceReport, workload: ViewSourceWorkload) -> &ViewSourceSample {
    &report.launches[0].samples[workload_index(workload)]
}

fn operation_summary(
    report: &ViewSourceReport,
    workload: ViewSourceWorkload,
    operation: ViewSourceOperation,
) -> OperationSummary {
    OperationSummary {
        wall_time_microseconds: MetricSummary::from_values(operation_values(
            report,
            workload,
            operation,
            Metric::Wall,
        )),
        process_tree_cpu_time_microseconds: MetricSummary::from_values(operation_values(
            report,
            workload,
            operation,
            Metric::Cpu,
        )),
    }
}

#[derive(Clone, Copy)]
enum Metric {
    Wall,
    Cpu,
}

fn operation_values(
    report: &ViewSourceReport,
    workload: ViewSourceWorkload,
    operation: ViewSourceOperation,
    metric: Metric,
) -> Vec<f64> {
    report
        .launches
        .iter()
        .map(|launch| {
            let sample = &launch.samples[workload_index(workload)];
            let measurement = match operation {
                ViewSourceOperation::CompactConstruction => sample.compact,
                ViewSourceOperation::EagerFullContextConstruction => sample.eager_full_context,
                ViewSourceOperation::FirstFullContextTransition => {
                    sample.first_full_context_transition
                }
            };
            match metric {
                Metric::Wall => u64_to_f64(measurement.wall_time_microseconds),
                Metric::Cpu => u64_to_f64(measurement.process_tree_cpu_time_nanoseconds) / 1_000.0,
            }
        })
        .collect()
}

fn relative_change(baseline: f64, current: f64) -> Option<f64> {
    (baseline != 0.0).then_some((current - baseline) / baseline * 100.0)
}

const fn workload_index(workload: ViewSourceWorkload) -> usize {
    match workload {
        ViewSourceWorkload::ManyModifiedFiles => 0,
        ViewSourceWorkload::SparseLargeModifiedFile => 1,
    }
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
    use crate::view_source::{
        BenchmarkSource, CaseOrder, FixtureIdentity, OperationMeasurement, RunnerEnvironment,
        ViewSourceLaunch, ViewSourceSample,
    };

    fn report(walls: [u64; 3]) -> ViewSourceReport {
        let protocol = ViewSourceProtocol {
            independent_launches: 3,
            workloads: ViewSourceWorkload::ALL.to_vec(),
            case_order_by_launch: vec![
                CaseOrder {
                    launch: 1,
                    operations: ViewSourceOperation::ALL.to_vec(),
                },
                CaseOrder {
                    launch: 2,
                    operations: vec![
                        ViewSourceOperation::EagerFullContextConstruction,
                        ViewSourceOperation::FirstFullContextTransition,
                        ViewSourceOperation::CompactConstruction,
                    ],
                },
                CaseOrder {
                    launch: 3,
                    operations: vec![
                        ViewSourceOperation::FirstFullContextTransition,
                        ViewSourceOperation::CompactConstruction,
                        ViewSourceOperation::EagerFullContextConstruction,
                    ],
                },
            ],
            process_tree_cpu_accounting: "accounting".into(),
            measurement_scope: "scope".into(),
            sparse_source_line_count: 20_000,
            sparse_change_stride: 200,
        };
        let fixtures = ViewSourceWorkload::ALL
            .into_iter()
            .map(|workload| FixtureIdentity {
                workload,
                name: workload.to_string(),
                range: "HEAD~1..HEAD".into(),
                source_sha256: "source".into(),
                file_count: 1,
                modified_file_count: 1,
                compact_diff_rows: 10,
                compact_diff_bytes: 100,
                full_context_diff_rows: 20,
                full_context_diff_bytes: 200,
                compact_semantic_sha256: format!("compact-{workload}"),
                full_semantic_sha256: format!("full-{workload}"),
            })
            .collect::<Vec<_>>();
        let launches = walls
            .into_iter()
            .enumerate()
            .map(|(index, wall)| ViewSourceLaunch {
                launch: index + 1,
                samples: fixtures
                    .iter()
                    .map(|fixture| ViewSourceSample {
                        workload: fixture.workload,
                        compact: measurement(wall),
                        eager_full_context: measurement(wall * 2),
                        first_full_context_transition: measurement(wall + 5),
                        compact_cache_weight_bytes: 100,
                        full_context_cache_weight_bytes: 200,
                        compact_semantic_sha256: fixture.compact_semantic_sha256.clone(),
                        full_semantic_sha256: fixture.full_semantic_sha256.clone(),
                    })
                    .collect(),
            })
            .collect();
        ViewSourceReport {
            format_version: REPORT_FORMAT_VERSION,
            benchmark: BENCHMARK_NAME.into(),
            source: BenchmarkSource {
                commit: "0123456789012345678901234567890123456789".into(),
                invocation: "just bench-view-source".into(),
                profile: "release".into(),
            },
            fixtures,
            protocol,
            runner: RunnerEnvironment {
                operating_system: "Linux".into(),
                kernel_release: "kernel".into(),
                architecture: "x86_64".into(),
                cpu_model: "cpu".into(),
                logical_cpu_count: 8,
                rustc_version: "rustc".into(),
                cargo_version: "cargo".into(),
                git_version: "git".into(),
            },
            launches,
        }
    }

    fn measurement(wall: u64) -> OperationMeasurement {
        OperationMeasurement {
            wall_time_microseconds: wall,
            process_tree_cpu_time_nanoseconds: wall * 1_000,
        }
    }

    #[test]
    fn report_requires_three_ordered_launches() {
        let mut report = report([10, 20, 30]);
        report.launches[1].launch = 3;

        assert_eq!(
            validate_report(&report),
            Err(ComparisonError::InvalidLaunchSequence {
                report: ReportRole::Current
            })
        );
    }

    #[test]
    fn report_rejects_semantic_drift_between_launches() {
        let mut report = report([10, 20, 30]);
        report.launches[2].samples[0].compact_semantic_sha256 = "drift".into();

        assert_eq!(
            validate_report(&report),
            Err(ComparisonError::InconsistentSemanticEvidence {
                report: ReportRole::Current
            })
        );
    }

    #[test]
    fn comparison_uses_the_three_launch_median() {
        let baseline = report([10, 30, 20]);
        let current = report([20, 60, 40]);

        let comparison = compare_reports(&baseline, &current).unwrap();
        let compact = comparison.workloads[0]
            .operations
            .iter()
            .find(|operation| operation.operation == ViewSourceOperation::CompactConstruction)
            .unwrap();

        assert!((compact.wall_time_microseconds.baseline.median - 20.0).abs() < f64::EPSILON);
        assert!((compact.wall_time_microseconds.current.median - 40.0).abs() < f64::EPSILON);
        assert!(
            compact
                .wall_time_microseconds
                .relative_change_percent
                .is_some_and(|change| (change - 100.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn descriptor_preflight_rejects_a_changed_fixture() {
        let baseline = report([10, 20, 30]);
        let mut descriptor = ViewSourceDescriptor {
            format_version: baseline.format_version,
            benchmark: baseline.benchmark.clone(),
            profile: baseline.source.profile.clone(),
            fixtures: baseline.fixtures.clone(),
            protocol: baseline.protocol.clone(),
            runner: baseline.runner.clone(),
        };
        descriptor.fixtures[0].source_sha256 = "changed".into();

        assert_eq!(
            ensure_compatible(&baseline, &descriptor),
            Err(ComparisonError::IncompatibleFixtures)
        );
    }
}
