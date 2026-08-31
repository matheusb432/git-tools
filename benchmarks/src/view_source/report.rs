use serde::{Deserialize, Serialize};

pub const REPORT_FORMAT_VERSION: u32 = 1;
pub const BENCHMARK_NAME: &str = "viewer-demand-loaded-full-context-source";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewSourceWorkload {
    ManyModifiedFiles,
    SparseLargeModifiedFile,
}

impl ViewSourceWorkload {
    pub const ALL: [Self; 2] = [Self::ManyModifiedFiles, Self::SparseLargeModifiedFile];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ManyModifiedFiles => "many-modified-files",
            Self::SparseLargeModifiedFile => "sparse-large-modified-file",
        }
    }
}

impl std::fmt::Display for ViewSourceWorkload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewSourceOperation {
    CompactConstruction,
    EagerFullContextConstruction,
    FirstFullContextTransition,
}

impl ViewSourceOperation {
    pub const ALL: [Self; 3] = [
        Self::CompactConstruction,
        Self::EagerFullContextConstruction,
        Self::FirstFullContextTransition,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CompactConstruction => "compact-construction",
            Self::EagerFullContextConstruction => "eager-full-context-construction",
            Self::FirstFullContextTransition => "first-full-context-transition",
        }
    }
}

impl std::fmt::Display for ViewSourceOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CaseOrder {
    pub launch: usize,
    pub operations: Vec<ViewSourceOperation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureIdentity {
    pub workload: ViewSourceWorkload,
    pub name: String,
    pub range: String,
    pub source_sha256: String,
    pub file_count: usize,
    pub modified_file_count: usize,
    pub compact_diff_rows: usize,
    pub compact_diff_bytes: u64,
    pub full_context_diff_rows: usize,
    pub full_context_diff_bytes: u64,
    pub compact_semantic_sha256: String,
    pub full_semantic_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BenchmarkSource {
    pub commit: String,
    pub invocation: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSourceProtocol {
    pub independent_launches: usize,
    pub workloads: Vec<ViewSourceWorkload>,
    pub case_order_by_launch: Vec<CaseOrder>,
    pub process_tree_cpu_accounting: String,
    pub measurement_scope: String,
    pub sparse_source_line_count: usize,
    pub sparse_change_stride: usize,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSourceDescriptor {
    pub format_version: u32,
    pub benchmark: String,
    pub profile: String,
    pub fixtures: Vec<FixtureIdentity>,
    pub protocol: ViewSourceProtocol,
    pub runner: RunnerEnvironment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationMeasurement {
    pub wall_time_microseconds: u64,
    pub process_tree_cpu_time_nanoseconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSourceSample {
    pub workload: ViewSourceWorkload,
    pub compact: OperationMeasurement,
    pub eager_full_context: OperationMeasurement,
    pub first_full_context_transition: OperationMeasurement,
    pub compact_cache_weight_bytes: u64,
    pub full_context_cache_weight_bytes: u64,
    pub compact_semantic_sha256: String,
    pub full_semantic_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSourceLaunch {
    pub launch: usize,
    pub samples: Vec<ViewSourceSample>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaunchFragment {
    pub source: BenchmarkSource,
    pub descriptor: ViewSourceDescriptor,
    pub launch: ViewSourceLaunch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSourceReport {
    pub format_version: u32,
    pub benchmark: String,
    pub source: BenchmarkSource,
    pub fixtures: Vec<FixtureIdentity>,
    pub protocol: ViewSourceProtocol,
    pub runner: RunnerEnvironment,
    pub launches: Vec<ViewSourceLaunch>,
}
