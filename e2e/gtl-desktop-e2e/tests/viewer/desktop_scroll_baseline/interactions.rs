use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use thirtyfour::WebDriver;

use super::{forward_diff, open_initial_diff_file, process_memory, runner_environment, wait};

#[derive(Serialize)]
struct InteractionReport {
    source_commit: String,
    workload: String,
    warm_tab_ms: Vec<f64>,
    menu_ms: Vec<f64>,
    path_menu_ms: Vec<f64>,
    copy_completion_ms: Vec<f64>,
    viewport_ms: Vec<f64>,
    uncached_viewport_ms: Vec<f64>,
    after_interactions: process_memory::DetailedMemory,
    idle: gtl_benchmarks::desktop_scroll::DesktopScrollReadinessSample,
    after_idle: process_memory::DetailedMemory,
}

pub(super) async fn measure(
    driver: &WebDriver,
    repository: &Path,
    data_root: &Path,
    name: &str,
    limit: &str,
    launch: usize,
) -> Result<()> {
    let viewport = sample(driver, "viewport", name, "").await?;
    let second = format!("{name}-second");
    forward_diff(repository, data_root, &second, limit)?;
    open_initial_diff_file(driver).await?;
    wait::until("second tab viewport", wait::ASSERTION_TIMEOUT, || async {
        let ready: bool = driver.execute(
            "return document.querySelector('[role=tab][aria-selected=true]')?.getAttribute('title') === arguments[0] && !!document.querySelector('[data-gtl-copy-text]');",
            vec![serde_json::json!(second)],
        ).await?.convert()?;
        Ok(ready.then_some(()))
    }).await?;
    let warm_tab_ms = sample(driver, "tab", name, &second).await?.milliseconds;
    let menu_ms = sample(driver, "menu", name, &second).await?.milliseconds;
    let path_menu_ms = sample(driver, "path", name, &second).await?.milliseconds;
    let copy_completion_ms = sample(driver, "copy", name, &second).await?.milliseconds;
    let after_interactions = process_memory::detailed_snapshot(data_root)?;
    let idle = process_memory::ReadinessProcessSampler::try_start(data_root)?
        .context("stable idle process tree")?;
    let started = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let idle = idle.finish(started.elapsed())?;
    let after_idle = process_memory::detailed_snapshot(data_root)?;
    let report = InteractionReport {
        source_commit: runner_environment::required_environment(
            "GTL_DESKTOP_SCROLL_SOURCE_COMMIT",
        )?,
        workload: name.to_owned(),
        warm_tab_ms,
        menu_ms,
        path_menu_ms,
        copy_completion_ms,
        viewport_ms: viewport.milliseconds,
        uncached_viewport_ms: viewport.uncached_viewport_ms,
        after_interactions,
        idle,
        after_idle,
    };
    let path = runner_environment::required_environment_path("GTL_DESKTOP_SCROLL_REPORT_PATH")?
        .with_extension(format!("{name}-{launch}.interactions.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
    println!("interaction measurements: {}", path.display());
    Ok(())
}

#[derive(Deserialize)]
struct Samples {
    milliseconds: Vec<f64>,
    uncached_viewport_ms: Vec<f64>,
}

async fn sample(driver: &WebDriver, kind: &str, first: &str, second: &str) -> Result<Samples> {
    let values: Samples = driver
        .execute_async(
            SCRIPT,
            vec![
                serde_json::json!(kind),
                serde_json::json!(first),
                serde_json::json!(second),
            ],
        )
        .await
        .with_context(|| format!("measure {kind} interactions"))?
        .convert()?;
    ensure!(
        values.milliseconds.len() == 20
            && values
                .milliseconds
                .iter()
                .chain(&values.uncached_viewport_ms)
                .all(|value| value.is_finite() && *value >= 0.0),
        "invalid {kind} measurements"
    );
    Ok(values)
}

const SCRIPT: &str = r#"
const [kind, first, second, done] = arguments;
const results = [];
const uncached = [];
let rowRequests = [];
const originalFetch = window.fetch;
window.fetch = function (input, options) {
    if (kind === "viewport" && String(input).includes("viewer_stream_rows_start")) {
        rowRequests.push(JSON.parse(options.body).request);
    }
    return originalFetch.apply(this, arguments);
};
const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
const root = () => document.querySelector('[data-gtl-diff-document]');
const ready = () => {
    const view = root();
    if (!view) return false;
    const box = view.getBoundingClientRect();
    const visible = element => {
        const row = element.getBoundingClientRect();
        return row.height > 0 && row.bottom > box.top && row.top < box.bottom;
    };
    return [...view.querySelectorAll('[data-gtl-diff-row]')].some(visible) &&
        [...view.querySelectorAll('[data-gtl-row-window]')].filter(visible)
            .every(element => element.getAttribute('aria-busy') === 'false');
};
const wait = async predicate => {
    const deadline = performance.now() + 10000;
    do {
        await frame();
        if (performance.now() > deadline) throw new Error(`${kind} did not become usable`);
    } while (!predicate());
    await frame();
};
(async () => {
    Object.defineProperty(navigator, 'clipboard', {
        configurable: true, value: { writeText: async () => { window.__gtlCopyComplete = true; } },
    });
    for (let iteration = 0; iteration < 20; iteration++) {
        await wait(ready);
        let action, complete;
        if (kind === 'tab') {
            const title = iteration % 2 === 0 ? first : second;
            const tab = [...document.querySelectorAll('[role=tab]')].find(tab => tab.title === title);
            const id = tab.closest('[data-viewer-tab-id]').getAttribute('data-viewer-tab-id');
            action = () => tab.click();
            complete = () => document.querySelector('[role=tab][aria-selected=true]')?.title === title &&
                root()?.getAttribute('data-view-identity')?.startsWith(`${id}:`) && ready();
        } else if (kind === 'viewport') {
            const view = root();
            const fraction = (iteration + 1) / 21;
            action = () => { view.scrollTop = (view.scrollHeight - view.clientHeight) * fraction; };
            complete = ready;
        } else if (kind === 'menu') {
            action = () => document.querySelector('[data-testid=viewer-menu-trigger]').click();
            complete = () => !!document.querySelector('[popover][aria-label="Viewer menu"]:popover-open');
        } else {
            root().scrollTop = 0;
            await wait(ready);
            const trigger = root().querySelector('button[aria-label="Copy file path"]');
            if (kind === 'path') {
                action = () => trigger.click();
                complete = () => !!document.querySelector('[popover][aria-label="Copy file path"]:popover-open');
            } else {
                window.__gtlCopyComplete = false;
                trigger.click();
                await wait(() => !!document.querySelector('[popover][aria-label="Copy file path"]:popover-open'));
                const popover = document.querySelector('[popover][aria-label="Copy file path"]:popover-open');
                const option = [...popover.querySelectorAll('button')].find(button => button.textContent.includes('Relative path'));
                action = () => option.click();
                complete = () => window.__gtlCopyComplete && !popover.matches(':popover-open');
            }
        }
        rowRequests = [];
        const start = performance.now();
        action();
        await wait(complete);
        const elapsed = performance.now() - start;
        results.push(elapsed);
        if (kind === 'viewport') {
            const view = root(), box = view.getBoundingClientRect();
            const neededFetch = [...view.querySelectorAll('[data-gtl-row-window]')].some(element => {
                const rectangle = element.getBoundingClientRect();
                if (rectangle.bottom <= box.top || rectangle.top >= box.bottom) return false;
                const file = `file-${element.closest('[data-file-index]').getAttribute('data-file-index')}`;
                const start = Number(element.getAttribute('data-gtl-row-window')) * 64;
                return rowRequests.some(request => request.file === file && request.row_range?.start === start);
            });
            if (neededFetch) uncached.push(elapsed);
        }
        if (kind === 'menu') document.querySelector('[data-testid=viewer-menu-trigger]').click();
        if (kind === 'path') root().querySelector('button[aria-label="Copy file path"]').click();
    }
    done({ milliseconds: results, uncached_viewport_ms: uncached });
})().catch(error => done({ error: String(error), samples: results }))
    .finally(() => { window.fetch = originalFetch; });
"#;
