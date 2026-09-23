use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const REPORT_FORMAT_VERSION: u32 = 2;
pub(crate) const REPORT_BYTES_MAX: u64 = 512 * 1_024;

pub(crate) const BENCHMARK_NAME: &str = "grpc-transport-get-push-confirmation-requirement";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Compatibility {
    pub(crate) build_profile: String,
    pub(crate) rpc: String,
    pub(crate) launch_count: usize,
    pub(crate) total_request_count: usize,
    pub(crate) warmup_request_count: usize,
    pub(crate) measured_request_count: usize,
    pub(crate) protobuf_sha256: String,
    pub(crate) ghz_config_sha256: String,
    pub(crate) resource_bounds: String,
    pub(crate) runner: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Transport {
    NamedPipe,
    Uds,
}

impl std::fmt::Display for Transport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NamedPipe => "named-pipe",
            Self::Uds => "uds",
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Report {
    pub(crate) format_version: u32,
    pub(crate) benchmark: String,
    pub(crate) source_commit: String,
    pub(crate) transport: Transport,
    pub(crate) compatibility: Compatibility,
    pub(crate) launches: Vec<LaunchMeasurement>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct LaunchMeasurement {
    pub(crate) request_latency_nanoseconds: Vec<u64>,
    pub(crate) requests_per_second: f64,
    pub(crate) server_process_tree_cpu_milliseconds: u64,
    pub(crate) peak_server_and_client_process_tree_rss_bytes: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct Summary {
    pub(crate) latency_p50_nanoseconds: f64,
    pub(crate) latency_p95_nanoseconds: f64,
    pub(crate) latency_p99_nanoseconds: f64,
    pub(crate) requests_per_second_median: f64,
    pub(crate) server_cpu_nanoseconds_per_request_median: f64,
    pub(crate) peak_process_tree_rss_bytes: f64,
}

impl Report {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.format_version == REPORT_FORMAT_VERSION && self.benchmark == BENCHMARK_NAME,
            "unsupported gRPC transport report format or benchmark"
        );
        let compatibility = &self.compatibility;
        ensure!(
            compatibility.launch_count > 0
                && compatibility.measured_request_count > 0
                && self.launches.len() == compatibility.launch_count,
            "gRPC transport report has invalid compatibility fields"
        );
        for (index, launch) in self.launches.iter().enumerate() {
            ensure!(
                launch.request_latency_nanoseconds.len() == compatibility.measured_request_count
                    && launch
                        .request_latency_nanoseconds
                        .iter()
                        .all(|latency| *latency > 0)
                    && launch.requests_per_second.is_finite()
                    && launch.requests_per_second > 0.0
                    && launch.server_process_tree_cpu_milliseconds > 0
                    && launch.peak_server_and_client_process_tree_rss_bytes > 0,
                "gRPC transport launch {} has invalid measurements",
                index + 1
            );
        }
        Ok(())
    }

    pub(crate) fn is_compatible_with(&self, compatibility: &Compatibility) -> bool {
        self.format_version == REPORT_FORMAT_VERSION && self.compatibility == *compatibility
    }
}

impl Summary {
    pub(crate) fn from_report(report: &Report) -> Result<Self> {
        report.validate()?;
        let mut latencies = report
            .launches
            .iter()
            .flat_map(|launch| launch.request_latency_nanoseconds.iter().copied())
            .collect::<Vec<_>>();
        latencies.sort_unstable();
        let request_count = u64_to_f64(
            u64::try_from(report.compatibility.total_request_count)
                .context("request count exceeds u64")?,
        );
        Ok(Self {
            latency_p50_nanoseconds: u64_to_f64(percentile(&latencies, 50)),
            latency_p95_nanoseconds: u64_to_f64(percentile(&latencies, 95)),
            latency_p99_nanoseconds: u64_to_f64(percentile(&latencies, 99)),
            requests_per_second_median: median(
                report
                    .launches
                    .iter()
                    .map(|launch| launch.requests_per_second),
            ),
            server_cpu_nanoseconds_per_request_median: median(report.launches.iter().map(
                |launch| {
                    u64_to_f64(launch.server_process_tree_cpu_milliseconds) * 1_000_000.0
                        / request_count
                },
            )),
            peak_process_tree_rss_bytes: report
                .launches
                .iter()
                .map(|launch| launch.peak_server_and_client_process_tree_rss_bytes)
                .max()
                .map(u64_to_f64)
                .context("gRPC transport report has no launches")?,
        })
    }
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = percentile.saturating_mul(sorted.len()).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn u64_to_f64(value: u64) -> f64 {
    let [
        byte_0,
        byte_1,
        byte_2,
        byte_3,
        byte_4,
        byte_5,
        byte_6,
        byte_7,
    ] = value.to_be_bytes();
    let high = u32::from_be_bytes([byte_0, byte_1, byte_2, byte_3]);
    let low = u32::from_be_bytes([byte_4, byte_5, byte_6, byte_7]);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn report_validation_and_summary_use_the_raw_launch_measurements() {
        let mut report = report();
        let summary = Summary::from_report(&report).unwrap();
        assert_close(summary.latency_p50_nanoseconds, 20.0);
        assert_close(summary.latency_p95_nanoseconds, 40.0);
        assert_close(summary.requests_per_second_median, 20.0);
        assert_close(summary.peak_process_tree_rss_bytes, 200.0);

        report.launches[1].request_latency_nanoseconds.pop();
        assert!(report.validate().is_err());
    }

    pub(crate) fn report() -> Report {
        let launch = |latencies, requests_per_second, cpu, rss| LaunchMeasurement {
            request_latency_nanoseconds: latencies,
            requests_per_second,
            server_process_tree_cpu_milliseconds: cpu,
            peak_server_and_client_process_tree_rss_bytes: rss,
        };
        Report {
            format_version: REPORT_FORMAT_VERSION,
            benchmark: BENCHMARK_NAME.to_owned(),
            source_commit: "a".repeat(40),
            transport: Transport::Uds,
            compatibility: Compatibility {
                build_profile: "release".to_owned(),
                rpc: "synthetic.Service.Read".to_owned(),
                launch_count: 2,
                total_request_count: 3,
                warmup_request_count: 1,
                measured_request_count: 2,
                protobuf_sha256: "b".repeat(64),
                ghz_config_sha256: "c".repeat(64),
                resource_bounds: "synthetic bounded worker".to_owned(),
                runner: "Example Linux; synthetic CPU; rustc".to_owned(),
            },
            launches: vec![
                launch(vec![10, 40], 20.0, 1, 100),
                launch(vec![20, 30], 10.0, 2, 200),
            ],
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < f64::EPSILON);
    }
}
