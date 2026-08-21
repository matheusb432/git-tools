#![cfg(target_os = "linux")]

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use gtl_benchmarks::desktop_scroll::{self, DesktopScrollManifest};
use gtl_web_contracts::test_ids;
use serde::Serialize;
use thirtyfour::{By, WebDriver, WebElement};

use crate::support::{self, wait};

#[path = "desktop_scroll_baseline/metrics.rs"]
mod metrics;
#[path = "desktop_scroll_baseline/process_memory.rs"]
mod process_memory;
#[path = "desktop_scroll_baseline/runner_environment.rs"]
mod runner_environment;

use metrics::{BrowserScrollSample, ScrollProtocol, ScrollSample};
use process_memory::ProcessMemorySnapshot;
use runner_environment::{RunnerEnvironment, SystemConditions};

const ASSERTION_TIMEOUT: Duration = Duration::from_secs(120);
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(45);
const WINDOW_WIDTH: u32 = 1_200;
const WINDOW_HEIGHT: u32 = 700;
const DIAGNOSTIC_BYTES_MAX: usize = 64 * 1024;
const BENCHMARK_NAME: &str = "desktop-scroll-production-viewer";
const VIEW_NAME: &str = "desktop-scroll-baseline";

const SCROLL_SCRIPT: &str = r"
const element = arguments[0];
const protocol = arguments[1];
const done = arguments[2];

try {
    const distance = protocol.distance_css_pixels;
    const step = protocol.step_css_pixels;
    const traversals = protocol.traversals;
    const scrollRange = element.scrollHeight - element.clientHeight;
    const result = {
        error: null,
        frame_timestamps_ms: [],
        scroll_height_css_pixels: element.scrollHeight,
        client_height_css_pixels: element.clientHeight,
        final_scroll_top_css_pixels: 0,
        inner_width_css_pixels: window.innerWidth,
        inner_height_css_pixels: window.innerHeight,
    };
    if (!Number.isInteger(distance) || !Number.isInteger(step) ||
        !Number.isInteger(traversals) || distance <= 0 || step <= 0 ||
        traversals <= 0 || distance % step !== 0 || scrollRange < distance) {
        result.error = `invalid fixed scroll protocol or range: range=${scrollRange}`;
        done(result);
        return;
    }

    element.scrollTop = 0;
    let completedTraversals = 0;
    let target = distance;
    requestAnimationFrame((startTimestamp) => {
        result.frame_timestamps_ms.push(startTimestamp);

        const advance = (timestamp) => {
            result.frame_timestamps_ms.push(timestamp);
            const direction = target === 0 ? -1 : 1;
            const next = element.scrollTop + direction * step;
            element.scrollTop = direction > 0
                ? Math.min(next, target)
                : Math.max(next, target);

            if (Math.abs(element.scrollTop - target) < 0.5) {
                completedTraversals += 1;
                if (completedTraversals === traversals) {
                    result.final_scroll_top_css_pixels = element.scrollTop;
                    done(result);
                    return;
                }
                target = target === 0 ? distance : 0;
            }
            requestAnimationFrame(advance);
        };

        requestAnimationFrame(advance);
    });
} catch (error) {
    done({ error: String(error) });
}
";

#[derive(Debug)]
struct BenchmarkInputs {
    output: PathBuf,
    launches: usize,
    source_commit: String,
    invocation: String,
    bounds: ResourceBounds,
}

#[derive(Debug, Serialize)]
struct ResourceBounds {
    cpu_quota_percent: usize,
    memory_max_bytes: u64,
    memory_swap_max_bytes: u64,
    tasks_max: usize,
    process_niceness: usize,
    cargo_jobs_max: usize,
    rayon_threads_max: usize,
    wall_time_minutes: u64,
    termination_grace_seconds: u64,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    format_version: u32,
    benchmark: &'static str,
    source: SourceEvidence,
    fixture_manifest: DesktopScrollManifest,
    protocol: BenchmarkProtocol,
    resource_bounds: ResourceBounds,
    runner: RunnerEnvironment,
    launches: Vec<LaunchReport>,
}

#[derive(Debug, Serialize)]
struct SourceEvidence {
    commit: String,
    invocation: String,
    profile: &'static str,
}

#[derive(Debug, Serialize)]
struct BenchmarkProtocol {
    independent_launches: usize,
    outer_window_width_pixels: u32,
    outer_window_height_pixels: u32,
    expected_layout: &'static str,
    expected_density: &'static str,
    readiness: &'static str,
    scroll: ScrollProtocol,
}

#[derive(Debug, Serialize)]
struct LaunchReport {
    launch: usize,
    conditions_before_launch: SystemConditions,
    outer_window: WindowRectangle,
    readiness_memory: ProcessMemorySnapshot,
    changed_files: ScrollSample,
    memory_after_changed_files: ProcessMemorySnapshot,
    commits: ScrollSample,
    memory_after_commits: ProcessMemorySnapshot,
}

#[derive(Debug, Serialize)]
struct WindowRectangle {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "run through `just desktop-scroll-benchmark` under the bounded release supervisor"]
async fn production_viewer_scrolls_realistic_files_and_commits() -> Result<()> {
    let inputs = BenchmarkInputs::from_environment()?;
    let fixture_evidence = desktop_scroll::verify_fixture(&desktop_scroll::fixture_root())
        .context("verify committed desktop scroll fixture before measurement")?;
    ensure!(
        fixture_evidence.manifest.workload.commit_count == desktop_scroll::COMMIT_COUNT,
        "fixture commit count drifted before desktop measurement"
    );
    ensure!(
        fixture_evidence.manifest.workload.distinct_file_count
            == desktop_scroll::DISTINCT_FILE_COUNT,
        "fixture file count drifted before desktop measurement"
    );
    let runner = runner_environment::describe()?;
    let mut launches = Vec::with_capacity(inputs.launches);

    for launch in 1..=inputs.launches {
        let conditions_before_launch = runner_environment::capture_conditions()?;
        let suite_name = format!("desktop-scroll-launch-{launch}");
        let launch_report = support::run_test_with_result(&suite_name, |session| {
            Box::pin(async move {
                measure_launch(session, launch, conditions_before_launch)
                    .await
                    .with_context(|| format!("measure independent desktop launch {launch}"))
            })
        })
        .await?;
        launches.push(launch_report);
    }

    let report_path = inputs.output.clone();
    let report = BenchmarkReport {
        format_version: 1,
        benchmark: BENCHMARK_NAME,
        source: SourceEvidence {
            commit: inputs.source_commit,
            invocation: inputs.invocation,
            profile: "release",
        },
        fixture_manifest: fixture_evidence.manifest,
        protocol: BenchmarkProtocol {
            independent_launches: inputs.launches,
            outer_window_width_pixels: WINDOW_WIDTH,
            outer_window_height_pixels: WINDOW_HEIGHT,
            expected_layout: "unified",
            expected_density: "compact",
            readiness: "active production view with 10 commits, 50 file summaries, and all 50 retained diff-file cards complete",
            scroll: ScrollProtocol::fixed(),
        },
        resource_bounds: inputs.bounds,
        runner,
        launches,
    };
    write_report(&report_path, &report)?;
    println!("desktop scroll benchmark report: {}", report_path.display());
    Ok(())
}

impl BenchmarkInputs {
    fn from_environment() -> Result<Self> {
        let launches = parse_environment("GTL_DESKTOP_SCROLL_LAUNCHES")?;
        ensure!(
            (3..=10).contains(&launches),
            "desktop scroll benchmark requires 3 through 10 independent launches"
        );
        Ok(Self {
            output: runner_environment::required_environment_path(
                "GTL_DESKTOP_SCROLL_REPORT_PATH",
            )?,
            launches,
            source_commit: runner_environment::required_environment(
                "GTL_DESKTOP_SCROLL_SOURCE_COMMIT",
            )?,
            invocation: runner_environment::required_environment("GTL_DESKTOP_SCROLL_INVOCATION")?,
            bounds: ResourceBounds {
                cpu_quota_percent: parse_environment("GTL_DESKTOP_SCROLL_CPU_QUOTA_PERCENT")?,
                memory_max_bytes: parse_environment("GTL_DESKTOP_SCROLL_MEMORY_MAX_BYTES")?,
                memory_swap_max_bytes: parse_environment(
                    "GTL_DESKTOP_SCROLL_MEMORY_SWAP_MAX_BYTES",
                )?,
                tasks_max: parse_environment("GTL_DESKTOP_SCROLL_TASKS_MAX")?,
                process_niceness: parse_environment("GTL_DESKTOP_SCROLL_PROCESS_NICENESS")?,
                cargo_jobs_max: parse_environment("GTL_DESKTOP_SCROLL_CARGO_JOBS_MAX")?,
                rayon_threads_max: parse_environment("GTL_DESKTOP_SCROLL_RAYON_THREADS_MAX")?,
                wall_time_minutes: parse_environment("GTL_DESKTOP_SCROLL_WALL_TIME_MINUTES")?,
                termination_grace_seconds: parse_environment(
                    "GTL_DESKTOP_SCROLL_TERMINATION_GRACE_SECONDS",
                )?,
            },
        })
    }
}

async fn measure_launch(
    session: &mut support::session::TestSession,
    launch: usize,
    conditions_before_launch: SystemConditions,
) -> Result<LaunchReport> {
    let repository = hydrate_repository(launch)?;
    let driver = session.driver();
    driver
        .set_window_rect(20, 20, WINDOW_WIDTH, WINDOW_HEIGHT)
        .await
        .context("set fixed desktop benchmark window")?;
    forward_fixture(&repository, session.data_root())?;
    wait_for_ready_view(driver).await?;
    driver
        .set_script_timeout(SCRIPT_TIMEOUT)
        .await
        .context("set benchmark animation-script timeout")?;

    let window = driver
        .get_window_rect()
        .await
        .context("read fixed desktop benchmark window")?;
    let readiness_memory = process_memory::snapshot(session.data_root())?;
    let changed_files_element =
        support::selectors::by_test_id(driver, test_ids::CHANGED_FILES_PANEL)
            .await
            .context("locate changed-files scroll panel")?;
    let changed_files = scroll_panel(driver, &changed_files_element, "changed-files").await?;
    let memory_after_changed_files = process_memory::snapshot(session.data_root())?;
    let commits_element = driver
        .find(By::Css("aside[aria-label='Commits'] > div"))
        .await
        .context("locate commits scroll panel")?;
    let commits = scroll_panel(driver, &commits_element, "commits").await?;
    let memory_after_commits = process_memory::snapshot(session.data_root())?;

    ensure!(
        changed_files.inner_width_css_pixels == commits.inner_width_css_pixels
            && changed_files.inner_height_css_pixels == commits.inner_height_css_pixels,
        "browser viewport changed between timed panel journeys"
    );

    Ok(LaunchReport {
        launch,
        conditions_before_launch,
        outer_window: WindowRectangle {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        },
        readiness_memory,
        changed_files,
        memory_after_changed_files,
        commits,
        memory_after_commits,
    })
}

fn hydrate_repository(launch: usize) -> Result<PathBuf> {
    let fixture_root = runner_environment::required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
    let repository = fixture_root
        .join("desktop-scroll-repositories")
        .join(format!("launch-{launch}"))
        .join("desktop-scroll-fixture");
    desktop_scroll::hydrate_fixture(&desktop_scroll::fixture_root(), &repository)
        .with_context(|| format!("hydrate deterministic fixture at {}", repository.display()))?;
    Ok(repository)
}

fn forward_fixture(repository: &Path, data_root: &Path) -> Result<()> {
    let cli = runner_environment::required_environment_path("GTL_E2E_CLI_BINARY")?;
    let output = Command::new(&cli)
        .args(["diff", "-l", "10", "-n", VIEW_NAME])
        .current_dir(repository)
        .env("GIT_TOOLS_DATA_DIR", data_root)
        .output()
        .with_context(|| format!("forward production diff through {}", cli.display()))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "production CLI fixture forwarding failed (exit {}): {}{}",
        output.status.code().unwrap_or(-1),
        bounded_diagnostic(&output.stdout),
        bounded_diagnostic(&output.stderr)
    )
}

async fn wait_for_ready_view(driver: &WebDriver) -> Result<()> {
    wait::until(
        "complete 10-commit, 50-file production desktop view",
        ASSERTION_TIMEOUT,
        || async {
            let active_tabs = driver
                .find_all(By::Css("[role='tab'][aria-selected='true']"))
                .await?;
            let Some(active_tab) = active_tabs.into_iter().next() else {
                return Ok(None);
            };
            if !active_tab.is_displayed().await?
                || !active_tab
                    .attr("title")
                    .await?
                    .unwrap_or_default()
                    .contains("desktop-scroll-fixture")
            {
                return Ok(None);
            }

            let documents = driver.find_all(By::Css("[data-gtl-diff-document]")).await?;
            let Some(document) = documents.into_iter().next() else {
                return Ok(None);
            };
            if !document.is_displayed().await?
                || document.attr("aria-busy").await?.as_deref() != Some("false")
                || document.attr("data-view-state").await?.as_deref() != Some("complete")
                || document.attr("data-chunks-complete").await?.as_deref() != Some("true")
                || document.attr("data-layout").await?.as_deref() != Some("unified")
                || document.attr("data-density").await?.as_deref() != Some("compact")
            {
                return Ok(None);
            }
            if document
                .find_all(By::Css("[data-gtl-diff-file]"))
                .await?
                .len()
                != desktop_scroll::DISTINCT_FILE_COUNT
            {
                return Ok(None);
            }

            let changed_files =
                support::selectors::by_test_id(driver, test_ids::CHANGED_FILES_PANEL).await?;
            let changed_files_text = changed_files.text().await?;
            if !changed_files.is_displayed().await?
                || !changed_files_text.contains("# 50 files")
                || !changed_files_text.contains("10 commits")
            {
                return Ok(None);
            }
            let commits = driver.find(By::Css("aside[aria-label='Commits']")).await?;
            if !commits.is_displayed().await?
                || commits
                    .find_all(By::Css("[data-gtl-action='copy-commit']"))
                    .await?
                    .len()
                    != desktop_scroll::COMMIT_COUNT
            {
                return Ok(None);
            }
            Ok(Some(()))
        },
    )
    .await
}

async fn scroll_panel(
    driver: &WebDriver,
    element: &WebElement,
    panel: &str,
) -> Result<ScrollSample> {
    let protocol = ScrollProtocol::fixed();
    let arguments = vec![
        element.to_json().context("encode scroll-panel element")?,
        serde_json::to_value(protocol).context("encode fixed scroll protocol")?,
    ];
    // WebDriver has no typed operation that can sequence requestAnimationFrame callbacks or
    // collect their browser timestamps. This single script owns the complete timed interval so
    // protocol round trips cannot contaminate the measurement.
    let result = driver
        .execute_async(SCROLL_SCRIPT, arguments)
        .await
        .with_context(|| format!("run browser-owned {panel} scroll interval"))?;
    let raw: BrowserScrollSample = result
        .convert()
        .with_context(|| format!("decode browser-owned {panel} scroll interval"))?;
    metrics::summarize(panel, protocol, &raw)
}

fn write_report(path: &Path, report: &BenchmarkReport) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("desktop scroll report path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create report directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary report in {}", parent.display()))?;
    serde_json::to_writer_pretty(&mut temporary, report).context("encode benchmark report")?;
    temporary
        .write_all(b"\n")
        .context("finish benchmark report")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync benchmark report")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish benchmark report {}", path.display()))?;
    Ok(())
}

fn parse_environment<T>(name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    runner_environment::required_environment(name)?
        .parse()
        .map_err(|error| anyhow!("parse {name}: {error}"))
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let end = bytes.len().min(DIAGNOSTIC_BYTES_MAX);
    let suffix = if bytes.len() > end {
        "...[truncated]"
    } else {
        ""
    };
    format!("{}{suffix}", String::from_utf8_lossy(&bytes[..end]))
}
