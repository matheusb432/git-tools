//! Black-box measurements for the production gRPC transport boundary.

mod comparison;
mod report;

pub use comparison::{
    GrpcTransportComparison, GrpcTransportComparisonError, GrpcTransportSummary, LaunchMetricDelta,
    LaunchMetricSummary, MetricDelta, ReportRole, compare_reports, summarize_report,
    validate_report,
};
pub use report::{
    BENCHMARK_NAME, BenchmarkSource, GhzMeasurement, GhzSample, GrpcTransportProtocol,
    GrpcTransportReport, GrpcTransportResourceBounds, GrpcTransportRunner, REPORT_FORMAT_VERSION,
    TransportKind, TransportLaunch,
};
