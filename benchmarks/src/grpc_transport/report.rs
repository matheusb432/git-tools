use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const REPORT_FORMAT_VERSION: u32 = 1;
pub const BENCHMARK_NAME: &str = "grpc-transport-get-push-confirmation-requirement";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BenchmarkSource {
    pub commit: String,
    pub invocation: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GrpcTransportProtocol {
    pub independent_launches: usize,
    pub rpc: String,
    pub proto_sha256: String,
    pub ghz_config_sha256: String,
    pub total_request_count: u64,
    pub warmup_request_count: u64,
    pub measured_request_count: u64,
    pub concurrency: u32,
    pub connections: u32,
    pub client_cpu_count: u32,
    pub request_timeout_nanoseconds: u64,
    pub ghz_wall_timeout_seconds: u64,
    pub memory_attribution: String,
    pub rss_sample_interval_microseconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportKind {
    Tcp,
    Uds,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp => formatter.write_str("tcp"),
            Self::Uds => formatter.write_str("uds"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GrpcTransportResourceBounds {
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
pub struct GrpcTransportRunner {
    pub operating_system: String,
    pub kernel_release: String,
    pub architecture: String,
    pub cpu_model: String,
    pub logical_cpu_count: usize,
    pub rustc_version: String,
    pub ghz_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GrpcTransportReport {
    pub format_version: u32,
    pub benchmark: String,
    pub source: BenchmarkSource,
    pub transport: TransportKind,
    pub protocol: GrpcTransportProtocol,
    pub resource_bounds: GrpcTransportResourceBounds,
    pub runner: GrpcTransportRunner,
    pub launches: Vec<TransportLaunch>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransportLaunch {
    pub launch: usize,
    pub server_cpu_time_nanoseconds: u64,
    pub peak_server_and_client_rss_bytes: u64,
    pub push_confirmation_required: bool,
    pub ghz: GhzMeasurement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GhzMeasurement {
    pub count: u64,
    #[serde(rename = "total")]
    pub total_nanoseconds: u64,
    #[serde(rename = "average")]
    pub average_nanoseconds: u64,
    #[serde(rename = "fastest")]
    pub fastest_nanoseconds: u64,
    #[serde(rename = "slowest")]
    pub slowest_nanoseconds: u64,
    pub rps: f64,
    #[serde(rename = "errorDistribution")]
    pub error_distribution: BTreeMap<String, u64>,
    #[serde(rename = "statusCodeDistribution")]
    pub status_code_distribution: BTreeMap<String, u64>,
    pub details: Vec<GhzSample>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GhzSample {
    pub timestamp: String,
    #[serde(rename = "latency")]
    pub latency_nanoseconds: u64,
    pub error: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_ghz_measurement_excludes_request_metadata() {
        let raw = r#"{
            "options": {"metadata": {"authorization": "Bearer benchmark-secret"}},
            "count": 1,
            "total": 100,
            "average": 100,
            "fastest": 100,
            "slowest": 100,
            "rps": 10.0,
            "errorDistribution": {},
            "statusCodeDistribution": {"OK": 1},
            "details": [{
                "timestamp": "2026-08-27T00:00:00Z",
                "latency": 100,
                "error": "",
                "status": "OK"
            }]
        }"#;

        let measurement: GhzMeasurement = serde_json::from_str(raw).unwrap();
        let retained = serde_json::to_string(&measurement).unwrap();

        assert!(!retained.contains("benchmark-secret"));
        assert!(!retained.contains("options"));
    }
}
