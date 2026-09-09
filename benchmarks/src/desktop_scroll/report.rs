use serde::{Deserialize, Serialize};

use super::DesktopScrollManifest;

pub const REPORT_FORMAT_VERSION: u32 = 7;
pub const BENCHMARK_NAME: &str = "desktop-scroll-production-viewer";
const DISTANCE_CSS_PIXELS: u32 = 160;
const STEP_CSS_PIXELS: u32 = 8;
const TRAVERSALS: u32 = 10;
const DOCUMENT_DISTANCE_CSS_PIXELS: u32 = 20_000;
const DOCUMENT_STEP_CSS_PIXELS: u32 = 200;
const DOCUMENT_TRAVERSALS: u32 = 4;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopScrollReport {
    pub format_version: u32,
    pub benchmark: String,
    pub source: DesktopScrollSource,
    pub fixture_manifest: DesktopScrollManifest,
    pub protocol: DesktopScrollBenchmarkProtocol,
    pub resource_bounds: DesktopScrollResourceBounds,
    pub runner: DesktopScrollRunner,
    pub launches: Vec<DesktopScrollLaunch>,
    pub single_file_launches: Vec<DesktopScrollSingleFileLaunch>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollSource {
    pub commit: String,
    pub invocation: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollResourceBounds {
    pub cpu_quota_percent: usize,
    pub memory_max_bytes: u64,
    pub memory_swap_max_bytes: u64,
    pub tasks_max: usize,
    pub process_niceness: usize,
    pub cargo_jobs_max: usize,
    pub rayon_threads_max: usize,
    pub wall_time_minutes: u64,
    pub termination_grace_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollBenchmarkProtocol {
    pub independent_launches: usize,
    pub outer_window_width_pixels: u32,
    pub outer_window_height_pixels: u32,
    pub expected_layout: String,
    pub expected_density: String,
    pub readiness: String,
    pub memory_attribution: String,
    pub process_cpu_clock_ticks_per_second: u64,
    pub script_timeout_seconds: u64,
    pub side_panel_scroll: ScrollProtocol,
    pub diff_document_scroll: ScrollProtocol,
    pub single_file: DesktopScrollSingleFileWorkload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollSingleFileWorkload {
    pub file_count: usize,
    pub source_line_count: usize,
    pub expected_diff_row_count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScrollProtocol {
    pub distance_css_pixels: u32,
    pub step_css_pixels: u32,
    pub traversals: u32,
}

impl ScrollProtocol {
    #[must_use]
    pub const fn side_panel() -> Self {
        Self {
            distance_css_pixels: DISTANCE_CSS_PIXELS,
            step_css_pixels: STEP_CSS_PIXELS,
            traversals: TRAVERSALS,
        }
    }

    #[must_use]
    pub const fn diff_document() -> Self {
        Self {
            distance_css_pixels: DOCUMENT_DISTANCE_CSS_PIXELS,
            step_css_pixels: DOCUMENT_STEP_CSS_PIXELS,
            traversals: DOCUMENT_TRAVERSALS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollRunner {
    pub operating_system: String,
    pub kernel_release: String,
    pub architecture: String,
    pub cpu_model: String,
    pub logical_cpu_count: usize,
    pub rustc_version: String,
    pub cargo_version: String,
    pub git_version: String,
    pub tauri_driver_version: String,
    pub webkitgtk_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopScrollLaunch {
    pub launch: usize,
    pub conditions_before_launch: DesktopScrollSystemConditions,
    pub outer_window: DesktopScrollWindow,
    pub readiness: DesktopScrollReadinessSample,
    pub loading_frame_gaps_ms: Vec<f64>,
    pub diff_document: ScrollSample,
    pub memory_after_diff_document: DesktopScrollProcessMemory,
    pub changed_files: ScrollSample,
    pub memory_after_changed_files: DesktopScrollProcessMemory,
    pub commits: ScrollSample,
    pub memory_after_commits: DesktopScrollProcessMemory,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopScrollSingleFileLaunch {
    pub launch: usize,
    pub conditions_before_launch: DesktopScrollSystemConditions,
    pub outer_window: DesktopScrollWindow,
    pub readiness: DesktopScrollReadinessSample,
    pub loading_frame_gaps_ms: Vec<f64>,
    pub diff_document: ScrollSample,
    pub memory_after_diff_document: DesktopScrollProcessMemory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollReadinessSample {
    pub wall_time_milliseconds: u64,
    pub process_cpu_clock_ticks: u64,
    pub peak_memory: DesktopScrollProcessMemory,
    pub viewer_peak_rss_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopScrollSystemConditions {
    pub recorded_at_unix_milliseconds: u64,
    pub load_average_1_minute: f64,
    pub load_average_5_minutes: f64,
    pub load_average_15_minutes: f64,
    pub memory_available_bytes: u64,
    pub cpu_governors: Vec<String>,
    pub energy_performance_preferences: Vec<String>,
    pub external_power_online: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollWindow {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollProcessMemory {
    pub attribution: String,
    pub process_count: usize,
    pub rss_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScrollSample {
    pub panel: String,
    pub distance_css_pixels: u32,
    pub step_css_pixels: u32,
    pub traversals: u32,
    pub total_distance_css_pixels: u64,
    pub scroll_height_css_pixels: u64,
    pub client_height_css_pixels: u64,
    pub final_scroll_top_css_pixels: f64,
    pub inner_width_css_pixels: u32,
    pub inner_height_css_pixels: u32,
    pub frame_count: usize,
    pub frame_gap_count: usize,
    pub total_duration_ms: f64,
    pub mean_frame_gap_ms: f64,
    pub p50_frame_gap_ms: f64,
    pub p95_frame_gap_ms: f64,
    pub p99_frame_gap_ms: f64,
    pub maximum_frame_gap_ms: f64,
    pub frames_exceeding_33_ms: usize,
    pub frame_gaps_ms: Vec<f64>,
}
