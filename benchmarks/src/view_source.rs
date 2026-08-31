//! Production benchmark for demand-loaded full-context diff source.

mod comparison;
mod fixture;
mod measurement;
mod report;

pub use comparison::{
    ComparisonError, CurrentWorkloadSummary, HistoricalWorkloadComparison, MetricDelta,
    MetricSummary, ViewSourceComparison, compare_reports, ensure_compatible, summarize_current,
    validate_descriptor, validate_report,
};
pub use measurement::{describe, measure_launch};
pub use report::{
    BENCHMARK_NAME, BenchmarkSource, CaseOrder, FixtureIdentity, LaunchFragment,
    OperationMeasurement, REPORT_FORMAT_VERSION, RunnerEnvironment, ViewSourceDescriptor,
    ViewSourceLaunch, ViewSourceOperation, ViewSourceProtocol, ViewSourceReport, ViewSourceSample,
    ViewSourceWorkload,
};
