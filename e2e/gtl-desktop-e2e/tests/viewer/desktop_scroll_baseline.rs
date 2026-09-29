#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{BufWriter, Write as _},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use gtl_benchmarks::desktop_scroll::{
    self, DesktopScrollBenchmarkProtocol, DesktopScrollLaunch, DesktopScrollReport,
    DesktopScrollResourceBounds, DesktopScrollSingleFileLaunch, DesktopScrollSingleFileWorkload,
    DesktopScrollSource, DesktopScrollSystemConditions, DesktopScrollWindow, ScrollProtocol,
    ScrollSample,
};
use gtl_web_contracts::test_ids;
use serde::{Deserialize, Serialize};
use thirtyfour::{WebDriver, WebElement};

use crate::support::{self, wait};

#[path = "desktop_scroll_baseline/interactions.rs"]
mod interactions;
#[path = "desktop_scroll_baseline/metrics.rs"]
mod metrics;
#[path = "desktop_scroll_baseline/process_memory.rs"]
mod process_memory;
#[path = "desktop_scroll_baseline/runner_environment.rs"]
mod runner_environment;

use metrics::BrowserScrollSample;

const ASSERTION_TIMEOUT: Duration = Duration::from_secs(120);
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(120);
const WINDOW_WIDTH: u32 = 1_200;
const WINDOW_HEIGHT: u32 = 700;
const DIAGNOSTIC_BYTES_MAX: usize = 64 * 1024;
const VIEW_NAME: &str = "desktop-scroll-baseline";
const SINGLE_FILE_VIEW_NAME: &str = "desktop-scroll-single-file";
const DIFF_DOCUMENT_SELECTOR: &str = "[data-gtl-diff-document]";
const MANY_FILE_DIFF_ROW_COUNT: usize = 4_247;

const READINESS_SCRIPT: &str = r#"
const isVisible = (element) => {
    if (element === null) return false;
    const style = getComputedStyle(element);
    const rectangle = element.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' &&
        rectangle.width > 0 && rectangle.height > 0;
};
const activeTabs = [...document.querySelectorAll("[role='tab'][aria-selected='true']")];
const activeTab = activeTabs[0] ?? null;
const documents = [...document.querySelectorAll('[data-gtl-diff-document]')];
const diffDocument = documents[0] ?? null;
const changedFiles = document.querySelector(arguments[0]);
const commitPanels = [...document.querySelectorAll(arguments[1])];
const commits = commitPanels.find(isVisible) ?? null;
const box = diffDocument?.getBoundingClientRect();
const inViewport = element => {
    const rectangle = element.getBoundingClientRect();
    return rectangle.bottom > box.top && rectangle.top < box.bottom && rectangle.height > 0;
};
const viewportReady = box !== undefined &&
    [...diffDocument.querySelectorAll('[data-gtl-diff-row]')].some(inViewport) &&
    [...diffDocument.querySelectorAll('[data-gtl-row-window]')]
        .filter(inViewport).every(window => window.getAttribute('aria-busy') === 'false');

return {
    active_tab_count: activeTabs.length,
    active_tab_title: activeTab?.textContent?.trim() ?? null,
    visible_active_tab_count: activeTabs.filter(isVisible).length,
    document_count: documents.length,
    visible_document_count: documents.filter(isVisible).length,
    document_aria_busy: diffDocument?.getAttribute('aria-busy') ?? null,
    document_view_state: diffDocument?.getAttribute('data-view-state') ?? null,
    document_chunks_complete: diffDocument?.getAttribute('data-chunks-complete') ?? null,
    document_layout: diffDocument?.getAttribute('data-layout') ?? null,
    document_density: diffDocument?.getAttribute('data-density') ?? null,
    diff_file_count: Number(diffDocument?.getAttribute('data-total-files') ??
        diffDocument?.querySelectorAll('[data-gtl-diff-file]').length ?? 0),
    viewport_ready: viewportReady,
    changed_file_count: changedFiles?.querySelectorAll('[data-file-target]').length ?? 0,
    visible_changed_files_count: Number(isVisible(changedFiles)),
    commit_panel_count: commitPanels.length,
    visible_commit_panel_count: commitPanels.filter(isVisible).length,
    commit_count: commits?.querySelectorAll('article[data-gtl-hover-popover-target]').length ?? 0,
};
"#;

const VISIBLE_ELEMENT_SCRIPT: &str = r"
const element = [...document.querySelectorAll(arguments[0])].find((candidate) => {
    const style = getComputedStyle(candidate);
    const rectangle = candidate.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' &&
        rectangle.width > 0 && rectangle.height > 0;
});
return element ?? null;
";

const DIFF_ROW_COUNT_SCRIPT: &str = r"
const diffDocument = [...document.querySelectorAll('[data-gtl-diff-document]')].find((candidate) => {
    const style = getComputedStyle(candidate);
    const rectangle = candidate.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' &&
        rectangle.width > 0 && rectangle.height > 0;
});
return Number(diffDocument?.getAttribute('data-total-rows') ??
    diffDocument?.querySelectorAll('[data-gtl-diff-row]').length ?? 0);
";

const EXPAND_DIFF_FILES_SCRIPT: &str = r#"
const diffDocument = [...document.querySelectorAll('[data-gtl-diff-document]')].find((candidate) => {
    const style = getComputedStyle(candidate);
    const rectangle = candidate.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' &&
        rectangle.width > 0 && rectangle.height > 0;
});
const files = [...(diffDocument?.querySelectorAll('[data-gtl-diff-file]') ?? [])];
if (diffDocument?.hasAttribute('data-total-files')) {
    const collapse = document.querySelector("button[aria-label='Collapse all diffs']");
    if (collapse) collapse.click();
    return Number(diffDocument.getAttribute('data-total-files'));
}
for (const file of files) {
    if (!file.open) file.firstElementChild?.click();
}
return files.length;
"#;

const DIFF_DOCUMENT_SCROLL_READY_SCRIPT: &str = r"
const diffDocument = [...document.querySelectorAll('[data-gtl-diff-document]')].find((candidate) => {
    const style = getComputedStyle(candidate);
    const rectangle = candidate.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' &&
        rectangle.width > 0 && rectangle.height > 0;
});
if (diffDocument === undefined) return false;
const files = [...diffDocument.querySelectorAll('[data-gtl-diff-file]')];
const total = Number(diffDocument.getAttribute('data-total-files') ?? files.length);
return total === arguments[0] && files.length > 0 && files.every((file) => file.open) &&
    diffDocument.scrollHeight - diffDocument.clientHeight >= arguments[1];
";

const START_LOADING_FRAMES_SCRIPT: &str = r"
const sample = { frame_gaps_ms: [], complete: false, error: null };
window.__gtlLoadingFrames = sample;
const deadline = performance.now() + 120000;
let previous = null;
let started = false;
let completing = false;
const observe = (timestamp) => {
    if (timestamp > deadline || sample.frame_gaps_ms.length >= 20000) {
        sample.error = 'loading frame sampler exceeded its bound';
        return;
    }
    const diffDocument = document.querySelector('[data-gtl-diff-document]');
    if (diffDocument !== null) started = true;
    if (started && previous !== null) sample.frame_gaps_ms.push(timestamp - previous);
    previous = timestamp;
    if (completing) {
        sample.complete = true;
        return;
    }
    if (diffDocument !== null) {
        const box = diffDocument.getBoundingClientRect();
        const inViewport = element => {
            const rectangle = element.getBoundingClientRect();
            return rectangle.bottom > box.top && rectangle.top < box.bottom && rectangle.height > 0;
        };
        completing = [...diffDocument.querySelectorAll('[data-gtl-diff-row]')].some(inViewport) &&
            [...diffDocument.querySelectorAll('[data-gtl-row-window]')]
                .filter(inViewport).every(window => window.getAttribute('aria-busy') === 'false');
    }
    requestAnimationFrame(observe);
};
requestAnimationFrame(observe);
";

const FINISH_LOADING_FRAMES_SCRIPT: &str = r"
const sample = window.__gtlLoadingFrames;
return sample?.complete || sample?.error ? sample : null;
";

#[derive(Deserialize)]
struct LoadingFrameSample {
    frame_gaps_ms: Vec<f64>,
    error: Option<String>,
}

async fn start_loading_measurement(
    driver: &WebDriver,
    data_root: &Path,
) -> Result<process_memory::ReadinessProcessSampler> {
    let sampler = wait::until(
        "one stable top-level viewer and server process tree",
        ASSERTION_TIMEOUT,
        || async { process_memory::ReadinessProcessSampler::try_start(data_root) },
    )
    .await?;
    driver
        .execute(START_LOADING_FRAMES_SCRIPT, Vec::new())
        .await
        .context("start loading animation-frame sampler")?;
    Ok(sampler)
}

async fn finish_loading_frames(driver: &WebDriver) -> Result<Vec<f64>> {
    let sample: LoadingFrameSample = wait::until(
        "final loading animation frame",
        ASSERTION_TIMEOUT,
        || async {
            driver
                .execute(FINISH_LOADING_FRAMES_SCRIPT, Vec::new())
                .await?
                .convert::<Option<LoadingFrameSample>>()
                .context("decode loading animation frames")
        },
    )
    .await?;
    ensure!(
        sample.error.is_none(),
        "loading frame sampler: {:?}",
        sample.error
    );
    ensure!(
        !sample.frame_gaps_ms.is_empty()
            && sample
                .frame_gaps_ms
                .iter()
                .all(|gap| gap.is_finite() && *gap >= 0.0),
        "loading frame sampler returned missing or invalid frame gaps"
    );
    Ok(sample.frame_gaps_ms)
}

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
    bounds: DesktopScrollResourceBounds,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReadinessSnapshot {
    active_tab_count: usize,
    active_tab_title: Option<String>,
    visible_active_tab_count: usize,
    document_count: usize,
    visible_document_count: usize,
    document_aria_busy: Option<String>,
    document_view_state: Option<String>,
    document_chunks_complete: Option<String>,
    document_layout: Option<String>,
    document_density: Option<String>,
    diff_file_count: usize,
    viewport_ready: bool,
    changed_file_count: usize,
    visible_changed_files_count: usize,
    commit_panel_count: usize,
    visible_commit_panel_count: usize,
    commit_count: usize,
}

#[derive(Clone, Copy)]
struct ReadyViewExpectation {
    name: &'static str,
    file_count: usize,
    commit_count: usize,
    diff_row_count: usize,
}

impl ReadinessSnapshot {
    fn is_ready(&self, expectation: ReadyViewExpectation) -> bool {
        self.active_tab_count == 1
            && self.active_tab_title.as_deref() == Some(expectation.name)
            && self.visible_active_tab_count == 1
            && self.document_count == 1
            && self.visible_document_count == 1
            && self.viewport_ready
            && self.document_layout.as_deref() == Some("unified")
            && self.document_density.as_deref() == Some("compact")
            && self.diff_file_count == expectation.file_count
            && self.visible_changed_files_count == 1
            && self.changed_file_count == expectation.file_count
            && self.commit_panel_count >= 1
            && self.visible_commit_panel_count == 1
            && self.commit_count == expectation.commit_count
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "run through `just bench-scroll` under the bounded release supervisor"]
async fn production_viewer_scrolls_large_diff_workloads() -> Result<()> {
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
    ensure!(
        fixture_evidence.manifest.workload.compact_diff_rows
            == MANY_FILE_DIFF_ROW_COUNT + desktop_scroll::DISTINCT_FILE_COUNT,
        "fixture compact row count drifted before desktop measurement"
    );
    let runner = runner_environment::describe()?;
    let mut launches = Vec::with_capacity(inputs.launches);
    let mut single_file_launches = Vec::with_capacity(inputs.launches);

    for launch in 1..=inputs.launches {
        let conditions_before_launch = runner_environment::capture_conditions()?;
        let suite_name = format!("desktop-scroll-launch-{launch}");
        let launch_report =
            run_independent_launch(&suite_name, launch, conditions_before_launch).await?;
        launches.push(launch_report);
    }
    for launch in 1..=inputs.launches {
        let conditions_before_launch = runner_environment::capture_conditions()?;
        let suite_name = format!("desktop-scroll-single-file-launch-{launch}");
        let launch_report =
            run_single_file_launch(&suite_name, launch, conditions_before_launch).await?;
        single_file_launches.push(launch_report);
    }

    let report_path = inputs.output.clone();
    let report = DesktopScrollReport {
        format_version: desktop_scroll::REPORT_FORMAT_VERSION,
        benchmark: desktop_scroll::BENCHMARK_NAME.to_owned(),
        source: DesktopScrollSource {
            commit: inputs.source_commit,
            invocation: inputs.invocation,
            profile: "release".to_owned(),
        },
        fixture_manifest: fixture_evidence.manifest,
        protocol: DesktopScrollBenchmarkProtocol {
            independent_launches: inputs.launches,
            outer_window_width_pixels: WINDOW_WIDTH,
            outer_window_height_pixels: WINDOW_HEIGHT,
            expected_layout: "unified".to_owned(),
            expected_density: "compact".to_owned(),
            readiness: "usable initial viewport and complete file/commit metadata for the unchanged many-file and 20,005-row fixtures; logical row totals verified separately".to_owned(),
            memory_attribution: process_memory::ATTRIBUTION.to_owned(),
            process_cpu_clock_ticks_per_second: process_memory::clock_ticks_per_second()?,
            script_timeout_seconds: SCRIPT_TIMEOUT.as_secs(),
            side_panel_scroll: ScrollProtocol::side_panel(),
            diff_document_scroll: ScrollProtocol::diff_document(),
            single_file: DesktopScrollSingleFileWorkload {
                file_count: 1,
                source_line_count: desktop_scroll::SINGLE_FILE_SOURCE_LINE_COUNT,
                expected_diff_row_count: desktop_scroll::SINGLE_FILE_DIFF_ROW_COUNT,
            },
        },
        resource_bounds: inputs.bounds,
        runner,
        launches,
        single_file_launches,
    };
    write_report(&report_path, &report)?;
    println!("desktop scroll benchmark report: {}", report_path.display());
    Ok(())
}

async fn run_single_file_launch(
    suite_name: &str,
    launch: usize,
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollSingleFileLaunch> {
    support::run_test_with_result(suite_name, |session| {
        Box::pin(measure_single_file_launch_with_context(
            session,
            launch,
            conditions_before_launch,
        ))
    })
    .await
}

async fn measure_single_file_launch_with_context(
    session: &mut support::session::TestSession,
    launch: usize,
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollSingleFileLaunch> {
    measure_single_file_launch(session, launch, conditions_before_launch)
        .await
        .with_context(|| format!("measure independent single-file desktop launch {launch}"))
}

async fn run_independent_launch(
    suite_name: &str,
    launch: usize,
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollLaunch> {
    support::run_test_with_result(suite_name, |session| {
        Box::pin(measure_launch_with_context(
            session,
            launch,
            conditions_before_launch,
        ))
    })
    .await
}

async fn measure_launch_with_context(
    session: &mut support::session::TestSession,
    launch: usize,
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollLaunch> {
    measure_launch(session, launch, conditions_before_launch)
        .await
        .with_context(|| format!("measure independent desktop launch {launch}"))
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
            bounds: DesktopScrollResourceBounds {
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
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollLaunch> {
    let repository = hydrate_repository(launch)?;
    let driver = session.driver();
    driver
        .set_window_rect(20, 20, WINDOW_WIDTH, WINDOW_HEIGHT)
        .await
        .context("set fixed desktop benchmark window")?;
    let mut readiness_process_sampler =
        start_loading_measurement(driver, session.data_root()).await?;
    let readiness_started_at = Instant::now();
    forward_fixture(&repository, session.data_root())?;
    open_initial_diff_file(driver).await?;
    let expectation = ReadyViewExpectation {
        name: VIEW_NAME,
        file_count: desktop_scroll::DISTINCT_FILE_COUNT,
        commit_count: desktop_scroll::COMMIT_COUNT,
        diff_row_count: MANY_FILE_DIFF_ROW_COUNT,
    };
    wait_for_ready_view(driver, &mut readiness_process_sampler, expectation).await?;
    let readiness = readiness_process_sampler.finish(readiness_started_at.elapsed())?;
    let loading_frame_gaps_ms = finish_loading_frames(driver).await?;
    verify_diff_row_count(driver, expectation).await?;
    prepare_scroll(driver).await?;
    let window = driver.get_window_rect().await?;
    expand_diff_files_for_scroll(driver, expectation).await?;
    let diff_document_element = visible_element(
        driver,
        DIFF_DOCUMENT_SELECTOR,
        "diff-document scroll region",
    )
    .await?;
    let diff_document = scroll_element(
        driver,
        &diff_document_element,
        "diff-document",
        ScrollProtocol::diff_document(),
    )
    .await?;
    let memory_after_diff_document = process_memory::snapshot(session.data_root())?;
    let changed_files_element = visible_element(
        driver,
        test_ids::CHANGED_FILES_PANEL.selector(),
        "changed-files scroll panel",
    )
    .await?;
    let changed_files = scroll_element(
        driver,
        &changed_files_element,
        "changed-files",
        ScrollProtocol::side_panel(),
    )
    .await?;
    let memory_after_changed_files = process_memory::snapshot(session.data_root())?;
    let commits_element = visible_element(
        driver,
        test_ids::COMMITS_PANEL.selector(),
        "commits scroll panel",
    )
    .await?;
    let commits = scroll_element(
        driver,
        &commits_element,
        "commits",
        ScrollProtocol::side_panel(),
    )
    .await?;
    let memory_after_commits = process_memory::snapshot(session.data_root())?;
    interactions::measure(
        driver,
        &repository,
        session.data_root(),
        VIEW_NAME,
        "10",
        launch,
    )
    .await?;

    ensure!(
        changed_files.inner_width_css_pixels == commits.inner_width_css_pixels
            && changed_files.inner_height_css_pixels == commits.inner_height_css_pixels,
        "browser viewport changed between timed panel journeys"
    );

    Ok(DesktopScrollLaunch {
        launch,
        conditions_before_launch,
        outer_window: DesktopScrollWindow {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        },
        readiness,
        loading_frame_gaps_ms,
        diff_document,
        memory_after_diff_document,
        changed_files,
        memory_after_changed_files,
        commits,
        memory_after_commits,
    })
}

async fn measure_single_file_launch(
    session: &mut support::session::TestSession,
    launch: usize,
    conditions_before_launch: DesktopScrollSystemConditions,
) -> Result<DesktopScrollSingleFileLaunch> {
    let repository = create_single_file_repository(single_file_repository_path(launch)?)?;
    let driver = session.driver();
    driver
        .set_window_rect(20, 20, WINDOW_WIDTH, WINDOW_HEIGHT)
        .await
        .context("set fixed single-file benchmark window")?;
    let mut readiness_process_sampler =
        start_loading_measurement(driver, session.data_root()).await?;
    let readiness_started_at = Instant::now();
    forward_single_file_fixture(&repository, session.data_root())?;
    open_initial_diff_file(driver).await?;
    let expectation = ReadyViewExpectation {
        name: SINGLE_FILE_VIEW_NAME,
        file_count: 1,
        commit_count: 1,
        diff_row_count: desktop_scroll::SINGLE_FILE_DIFF_ROW_COUNT,
    };
    wait_for_ready_view(driver, &mut readiness_process_sampler, expectation).await?;
    let readiness = readiness_process_sampler.finish(readiness_started_at.elapsed())?;
    let loading_frame_gaps_ms = finish_loading_frames(driver).await?;
    verify_diff_row_count(driver, expectation).await?;
    prepare_scroll(driver).await?;
    let window = driver.get_window_rect().await?;
    expand_diff_files_for_scroll(driver, expectation).await?;
    let diff_document_element = visible_element(
        driver,
        DIFF_DOCUMENT_SELECTOR,
        "diff-document scroll region",
    )
    .await?;
    let diff_document = scroll_element(
        driver,
        &diff_document_element,
        "diff-document",
        ScrollProtocol::diff_document(),
    )
    .await?;
    let memory_after_diff_document = process_memory::snapshot(session.data_root())?;

    interactions::measure(
        driver,
        &repository,
        session.data_root(),
        SINGLE_FILE_VIEW_NAME,
        "1",
        launch,
    )
    .await?;

    Ok(DesktopScrollSingleFileLaunch {
        launch,
        conditions_before_launch,
        outer_window: DesktopScrollWindow {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        },
        readiness,
        loading_frame_gaps_ms,
        diff_document,
        memory_after_diff_document,
    })
}

async fn prepare_scroll(driver: &WebDriver) -> Result<()> {
    driver
        .set_script_timeout(SCRIPT_TIMEOUT)
        .await
        .context("set benchmark animation-script timeout")
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

fn single_file_repository_path(launch: usize) -> Result<PathBuf> {
    let fixture_root = runner_environment::required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
    Ok(fixture_root
        .join("desktop-scroll-single-file-repositories")
        .join(format!("launch-{launch}")))
}

/// Creates the 20,000-line single-file repository at `repository`.
pub(crate) fn create_single_file_repository(repository: PathBuf) -> Result<PathBuf> {
    ensure!(
        !repository.exists(),
        "single-file fixture already exists: {}",
        repository.display()
    );
    fs::create_dir_all(&repository)
        .with_context(|| format!("create single-file fixture at {}", repository.display()))?;
    fixture_git(&repository, &["init", "-q", "-b", "main"])?;
    fixture_git(
        &repository,
        &["config", "user.name", "Desktop Scroll Fixture"],
    )?;
    fixture_git(
        &repository,
        &["config", "user.email", "desktop-scroll@example.invalid"],
    )?;
    fs::write(
        repository.join("README.md"),
        "single-file desktop benchmark\n",
    )
    .context("write single-file fixture base")?;
    fixture_git(&repository, &["add", "README.md"])?;
    fixture_git(&repository, &["commit", "-q", "-m", "fixture: add base"])?;
    fixture_git(&repository, &["switch", "-q", "-c", "feature"])?;
    let source_path = repository.join("src/large.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .context("single-file fixture source has no parent")?,
    )
    .context("create single-file fixture source directory")?;
    let mut source = BufWriter::new(
        fs::File::create(&source_path).context("create single-file fixture source")?,
    );
    for line_number in 1..=desktop_scroll::SINGLE_FILE_SOURCE_LINE_COUNT {
        writeln!(
            source,
            "pub const ROW_{line_number:05}: &str = \"single-file-benchmark-{line_number:05}\";"
        )
        .context("write single-file fixture source")?;
    }
    source.flush().context("flush single-file fixture source")?;
    fixture_git(&repository, &["add", "src/large.rs"])?;
    fixture_git(
        &repository,
        &["commit", "-q", "-m", "fixture: add large source file"],
    )?;
    Ok(repository)
}

fn fixture_git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00+00:00")
        .output()
        .with_context(|| format!("run fixture Git {}", arguments.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "fixture Git {} failed (exit {}): {}{}",
        arguments.join(" "),
        output.status.code().unwrap_or(-1),
        bounded_diagnostic(&output.stdout),
        bounded_diagnostic(&output.stderr)
    )
}

fn forward_fixture(repository: &Path, data_root: &Path) -> Result<()> {
    forward_diff(repository, data_root, VIEW_NAME, "10")
}

pub(crate) fn forward_single_file_fixture(repository: &Path, data_root: &Path) -> Result<()> {
    forward_diff(repository, data_root, SINGLE_FILE_VIEW_NAME, "1")
}

fn forward_diff(repository: &Path, data_root: &Path, name: &str, limit: &str) -> Result<()> {
    let cli = runner_environment::required_environment_path("GTL_E2E_CLI_BINARY")?;
    let output = Command::new(&cli)
        .args(["diff", "-l", limit, "-n", name])
        .current_dir(repository)
        .env("GIT_TOOLS_DATA_DIR", data_root)
        .output()
        .with_context(|| format!("forward production diff through {}", cli.display()))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "production CLI fixture forwarding failed for {name} (exit {}): {}{}",
        output.status.code().unwrap_or(-1),
        bounded_diagnostic(&output.stdout),
        bounded_diagnostic(&output.stderr)
    )
}

async fn wait_for_ready_view(
    driver: &WebDriver,
    process_sampler: &mut process_memory::ReadinessProcessSampler,
    expectation: ReadyViewExpectation,
) -> Result<()> {
    let readiness = wait::until(
        "complete production desktop diff view",
        ASSERTION_TIMEOUT,
        || {
            let process_observation = process_sampler.observe();
            async move {
                process_observation?;
                let snapshot = readiness_snapshot(driver).await?;
                Ok(snapshot.is_ready(expectation).then_some(()))
            }
        },
    )
    .await;
    if let Err(error) = readiness {
        let diagnostic = readiness_diagnostic(driver)
            .await
            .unwrap_or_else(|diagnostic_error| format!("unavailable: {diagnostic_error:#}"));
        return Err(error).context(format!("last readiness observation: {diagnostic}"));
    }
    Ok(())
}

async fn open_initial_diff_file(driver: &WebDriver) -> Result<()> {
    wait::until("initial diff file", ASSERTION_TIMEOUT, || async {
        let opened: bool = driver
            .execute(
                r"
            const file = document.querySelector('[data-gtl-diff-file]');
            if (!file) return false;
            if (!file.open) file.firstElementChild?.click();
            return true;
            ",
                Vec::new(),
            )
            .await?
            .convert()?;
        Ok(opened.then_some(()))
    })
    .await
}

async fn verify_diff_row_count(
    driver: &WebDriver,
    expectation: ReadyViewExpectation,
) -> Result<()> {
    wait::until(
        "complete logical diff row count",
        ASSERTION_TIMEOUT,
        || async {
            let actual: usize = driver
                .execute(DIFF_ROW_COUNT_SCRIPT, Vec::new())
                .await?
                .convert()
                .context("decode production diff row count")?;
            Ok((actual == expectation.diff_row_count).then_some(()))
        },
    )
    .await
}

async fn expand_diff_files_for_scroll(
    driver: &WebDriver,
    expectation: ReadyViewExpectation,
) -> Result<()> {
    let expanded_file_count: usize = driver
        .execute(EXPAND_DIFF_FILES_SCRIPT, Vec::new())
        .await
        .context("expand every completed diff file before scrolling")?
        .convert()
        .context("decode expanded diff-file count")?;
    ensure!(
        expanded_file_count == expectation.file_count,
        "expanded {expanded_file_count} diff files, expected {}",
        expectation.file_count
    );
    let expand = driver
        .find_all(thirtyfour::By::Css("button[aria-label='Expand all diffs']"))
        .await?;
    if let Some(expand) = expand.first() {
        expand.click().await?;
    }
    let protocol = ScrollProtocol::diff_document();
    wait::until(
        "expanded diff document with the fixed scroll range",
        ASSERTION_TIMEOUT,
        || async {
            let ready: bool = driver
                .execute(
                    DIFF_DOCUMENT_SCROLL_READY_SCRIPT,
                    vec![
                        serde_json::json!(expectation.file_count),
                        serde_json::json!(protocol.distance_css_pixels),
                    ],
                )
                .await
                .context("observe expanded diff-document scroll range")?
                .convert()
                .context("decode expanded diff-document scroll readiness")?;
            Ok(ready.then_some(()))
        },
    )
    .await
}

async fn readiness_snapshot(driver: &WebDriver) -> Result<ReadinessSnapshot> {
    let result = driver
        .execute(
            READINESS_SCRIPT,
            vec![
                serde_json::Value::String(test_ids::CHANGED_FILES_PANEL.selector().to_owned()),
                serde_json::Value::String(test_ids::COMMITS_PANEL.selector().to_owned()),
            ],
        )
        .await
        .context("observe desktop scroll readiness")?;
    result
        .convert()
        .context("decode desktop scroll readiness observation")
}

async fn readiness_diagnostic(driver: &WebDriver) -> Result<String> {
    let snapshot = readiness_snapshot(driver).await?;
    serde_json::to_string(&snapshot).context("encode desktop scroll readiness diagnostic")
}

async fn visible_element(
    driver: &WebDriver,
    selector: &'static str,
    label: &str,
) -> Result<WebElement> {
    let result = driver
        .execute(
            VISIBLE_ELEMENT_SCRIPT,
            vec![serde_json::Value::String(selector.to_owned())],
        )
        .await
        .with_context(|| format!("locate visible {label}"))?;
    ensure!(!result.json().is_null(), "visible {label} is missing");
    result
        .element()
        .with_context(|| format!("decode visible {label}"))
}

async fn scroll_element(
    driver: &WebDriver,
    element: &WebElement,
    panel: &str,
    protocol: ScrollProtocol,
) -> Result<ScrollSample> {
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

fn write_report(path: &Path, report: &DesktopScrollReport) -> Result<()> {
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

#[cfg(test)]
fn complete_many_file_snapshot() -> ReadinessSnapshot {
    ReadinessSnapshot {
        active_tab_count: 1,
        active_tab_title: Some(VIEW_NAME.to_owned()),
        visible_active_tab_count: 1,
        document_count: 1,
        visible_document_count: 1,
        document_aria_busy: Some("false".to_owned()),
        document_view_state: Some("complete".to_owned()),
        document_chunks_complete: Some("true".to_owned()),
        document_layout: Some("unified".to_owned()),
        document_density: Some("compact".to_owned()),
        diff_file_count: desktop_scroll::DISTINCT_FILE_COUNT,
        viewport_ready: true,
        changed_file_count: desktop_scroll::DISTINCT_FILE_COUNT,
        visible_changed_files_count: 1,
        commit_panel_count: 1,
        visible_commit_panel_count: 1,
        commit_count: desktop_scroll::COMMIT_COUNT,
    }
}

#[cfg(test)]
const MANY_FILE_EXPECTATION: ReadyViewExpectation = ReadyViewExpectation {
    name: VIEW_NAME,
    file_count: desktop_scroll::DISTINCT_FILE_COUNT,
    commit_count: desktop_scroll::COMMIT_COUNT,
    diff_row_count: MANY_FILE_DIFF_ROW_COUNT,
};

#[test]
fn readiness_accepts_complete_production_dom_snapshot() {
    assert!(complete_many_file_snapshot().is_ready(MANY_FILE_EXPECTATION));
}

#[test]
fn readiness_waits_for_every_changed_file_entry() {
    let snapshot = ReadinessSnapshot {
        changed_file_count: desktop_scroll::DISTINCT_FILE_COUNT - 1,
        ..complete_many_file_snapshot()
    };

    assert!(!snapshot.is_ready(MANY_FILE_EXPECTATION));
}
