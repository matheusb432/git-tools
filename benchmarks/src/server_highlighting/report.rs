use serde::{Deserialize, Serialize};

pub const REPORT_FORMAT_VERSION: u32 = 3;
pub const BENCHMARK_NAME: &str = "server-highlighting-production-stream";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightWorkload {
    Plain,
    Rust,
    Mixed,
    Full,
}

impl HighlightWorkload {
    pub const ALL: [Self; 4] = [Self::Plain, Self::Rust, Self::Mixed, Self::Full];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Rust => "rust",
            Self::Mixed => "mixed",
            Self::Full => "full",
        }
    }
}

impl std::fmt::Display for HighlightWorkload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StreamTemperature {
    Cold,
    Warm,
}

impl StreamTemperature {
    pub const ALL: [Self; 2] = [Self::Cold, Self::Warm];
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureIdentity {
    pub workload: HighlightWorkload,
    pub name: String,
    pub source_sha256: String,
    pub file_count: usize,
    pub changed_line_count: usize,
    pub changed_bytes: u64,
    pub last_commit_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BenchmarkSource {
    pub commit: String,
    pub invocation: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HighlightProtocol {
    pub independent_launches: usize,
    pub workloads: Vec<HighlightWorkload>,
    pub temperatures: Vec<StreamTemperature>,
    pub residency_sequence: Vec<HighlightWorkload>,
    pub layout: String,
    pub density: String,
    pub stream_scope: String,
    pub rss_attribution: String,
    pub rss_sample_interval_microseconds: u64,
    pub process_cpu_clock_ticks_per_second: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceBounds {
    pub cpu_quota_percent: usize,
    pub memory_max_bytes: u64,
    pub memory_swap_max_bytes: u64,
    pub tasks_max: usize,
    pub process_niceness: usize,
    pub cargo_jobs_max: usize,
    pub wall_time_minutes: u64,
    pub termination_grace_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunnerEnvironment {
    pub operating_system: String,
    pub kernel_release: String,
    pub architecture: String,
    pub cpu_model: String,
    pub logical_cpu_count: usize,
    pub rustc_version: String,
    pub cargo_version: String,
    pub git_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SystemConditions {
    pub recorded_at_unix_milliseconds: u64,
    pub load_average_1_minute: f64,
    pub load_average_5_minutes: f64,
    pub load_average_15_minutes: f64,
    pub memory_available_bytes: u64,
    pub cpu_governors: Vec<String>,
    pub energy_performance_preferences: Vec<String>,
    pub external_power_online: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessMemory {
    pub rss: u64,
    pub high_water: u64,
    pub anonymous: u64,
    pub private_dirty: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StreamEvidence {
    pub message_count: usize,
    pub row_count: usize,
    pub encoded_bytes: usize,
    pub semantic_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StreamMeasurement {
    pub wall_time_microseconds: u64,
    pub server_cpu_time_nanoseconds: u64,
    pub server_cpu_clock_ticks: u64,
    pub memory_before: ProcessMemory,
    pub peak_rss_bytes: u64,
    pub memory_after: ProcessMemory,
    pub evidence: StreamEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HighlightSample {
    pub temperature: StreamTemperature,
    pub measurement: StreamMeasurement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HighlightLaunch {
    pub launch: usize,
    pub workload: HighlightWorkload,
    pub conditions_before_launch: SystemConditions,
    pub samples: Vec<HighlightSample>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResidencySample {
    pub workload: HighlightWorkload,
    pub measurement: StreamMeasurement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResidencyLaunch {
    pub launch: usize,
    pub conditions_before_launch: SystemConditions,
    pub samples: Vec<ResidencySample>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ServerHighlightingReport {
    pub format_version: u32,
    pub benchmark: String,
    pub source: BenchmarkSource,
    pub fixtures: Vec<FixtureIdentity>,
    pub protocol: HighlightProtocol,
    pub resource_bounds: ResourceBounds,
    pub runner: RunnerEnvironment,
    pub launches: Vec<HighlightLaunch>,
    pub residency_launches: Vec<ResidencyLaunch>,
}
