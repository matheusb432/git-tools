//! Production-boundary evidence for server-owned syntax highlighting.

mod comparison;
mod fixtures;
mod process;
mod report;
mod stream;

pub use comparison::{
    ComparisonError, MetricDelta, SampleComparison, ServerHighlightingComparison,
    WorkloadComparison, compare_reports, validate_report,
};
pub use fixtures::{
    FixtureError, MaterializedFixture, materialize_fixture, synthetic_fixture_identity,
};
pub use process::{
    ProcessRss, ProcessSample, ProcessSampleError, clock_ticks_per_second, parse_process_rss,
    parse_process_sample, read_process_rss, read_process_sample,
};
pub use report::{
    BENCHMARK_NAME, BenchmarkSource, FixtureIdentity, HighlightLaunch, HighlightProtocol,
    HighlightSample, HighlightWorkload, ProcessMemory, REPORT_FORMAT_VERSION, ResidencyLaunch,
    ResidencySample, ResourceBounds, RunnerEnvironment, ServerHighlightingReport, StreamEvidence,
    StreamMeasurement, StreamTemperature, SystemConditions,
};
pub use stream::{StreamValidationError, StreamValidator};
