use anyhow::{Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use super::{create_single_file_repository, forward_single_file_fixture, support, wait};

#[tokio::test(flavor = "multi_thread")]
async fn huge_file_search_and_selection_cross_unmounted_rows() -> Result<()> {
    support::run_test("viewer-virtualized-selection", |session| {
        Box::pin(run(session))
    })
    .await
}

async fn run(session: &mut support::session::TestSession) -> Result<()> {
    let repository = create_single_file_repository(100)?;
    let driver = session.driver();
    driver.set_window_rect(20, 20, 1_200, 700).await?;
    forward_single_file_fixture(&repository, session.data_root())?;
    wait_for_marker(driver, "single-file-benchmark-00001").await?;
    assert_bounded_rows(driver).await?;
    copy_across_unmounted_rows(driver).await?;
    copy_whole_document(driver).await?;
    driver
        .execute("getSelection().removeAllRanges();", Vec::new())
        .await?;
    driver
        .action_chain()
        .key_down(Key::Control)
        .send_keys("f")
        .key_up(Key::Control)
        .perform()
        .await?;
    driver
        .find(By::Id("viewer-diff-find-input"))
        .await?
        .send_keys("single-file-benchmark-20000")
        .await?;
    wait_for_marker(driver, "single-file-benchmark-20000").await?;
    assert_bounded_rows(driver).await?;
    assert_binary_row_ipc(driver).await?;
    rapid_scroll_cleans_up_delayed_starts(driver).await?;
    driver
        .find(By::Id("viewer-diff-find-input"))
        .await?
        .clear()
        .await?;
    driver
        .find(By::Id("viewer-diff-find-input"))
        .await?
        .send_keys("single-file-benchmark-20000")
        .await?;
    wait_for_marker(driver, "single-file-benchmark-20000").await?;
    retain_tab_presentation(session, &repository).await?;
    support::evidence::capture(driver, "viewer-virtualized-selection", true).await?;
    Ok(())
}

async fn retain_tab_presentation(
    session: &support::session::TestSession,
    repository: &std::path::Path,
) -> Result<()> {
    let driver = session.driver();
    super::forward_diff(repository, session.data_root(), "viewport-second", "1")?;
    support::wait_for_active_diff(driver, "viewport-second", "single-file-benchmark-00001").await?;
    driver
        .find(By::Css("button[aria-label='Collapse all']"))
        .await?
        .click()
        .await?;
    wait::until("second tab folded", wait::ASSERTION_TIMEOUT, || async {
        let closed: bool = driver.execute(
            "return document.querySelector('[role=tab][aria-selected=true]')?.title === 'viewport-second' && document.querySelector('[data-gtl-diff-file]')?.open === false;",
            Vec::new(),
        ).await?.convert()?;
        Ok(closed.then_some(()))
    }).await?;
    for _ in 0..2 {
        driver
            .find(By::Css("[role=tab][title='desktop-scroll-single-file']"))
            .await?
            .click()
            .await?;
        support::wait_for_active_diff(
            driver,
            "desktop-scroll-single-file",
            "single-file-benchmark-20000",
        )
        .await?;
        let restored: bool = driver.execute(
            "return document.querySelector('[data-gtl-diff-file]')?.open === true && document.querySelector('[data-gtl-diff-document]').scrollTop > 20000;",
            Vec::new(),
        ).await?.convert()?;
        ensure!(
            restored,
            "the first tab lost its expanded file or scroll anchor"
        );
        assert_bounded_rows(driver).await?;
        driver
            .find(By::Css("[role=tab][title='viewport-second']"))
            .await?
            .click()
            .await?;
        wait::until("second tab keeps its fold", wait::ASSERTION_TIMEOUT, || async {
            let closed: bool = driver.execute(
                "return document.querySelector('[role=tab][aria-selected=true]')?.title === 'viewport-second' && document.querySelector('[data-gtl-diff-file]')?.open === false;",
                Vec::new(),
            ).await?.convert()?;
            Ok(closed.then_some(()))
        }).await?;
    }
    Ok(())
}

async fn copy_across_unmounted_rows(driver: &WebDriver) -> Result<()> {
    driver
        .execute(
            r"
            const root = document.querySelector('[data-gtl-diff-document]');
            const first = root.querySelector('[data-gtl-copy-text]');
            const selection = getSelection();
            selection.selectAllChildren(first);
            window.__gtlSelectionStart = first;
            ",
            Vec::new(),
        )
        .await?;
    // Let selectionchange retain the endpoint before moving the viewport.
    driver
        .execute_async(
            "requestAnimationFrame(() => requestAnimationFrame(arguments[0]));",
            Vec::new(),
        )
        .await?;
    driver
        .execute(
            "document.querySelector('[data-gtl-diff-document]').scrollTop = 20000;",
            Vec::new(),
        )
        .await?;
    wait_for_marker(driver, "single-file-benchmark-01000").await?;
    assert_bounded_rows(driver).await?;
    let selected: bool = driver
        .execute(
            r"
            const root = document.querySelector('[data-gtl-diff-document]');
            const end = [...root.querySelectorAll('[data-gtl-copy-text]')]
                .find(line => line.textContent.includes('single-file-benchmark-01000'));
            const start = window.__gtlSelectionStart;
            if (!start?.isConnected || !end) return false;
            getSelection().setBaseAndExtent(start, 0, end, end.childNodes.length);
            Object.defineProperty(navigator, 'clipboard', {
                configurable: true,
                value: { writeText: async text => { window.__gtlCopiedSelection = text; } },
            });
            const data = new DataTransfer();
            const event = new ClipboardEvent('copy', { bubbles: true, cancelable: true, clipboardData: data });
            document.body.dispatchEvent(event);
            if (data.getData('text/plain')) window.__gtlCopiedSelection = data.getData('text/plain');
            return event.defaultPrevented;
            ",
            Vec::new(),
        )
        .await?
        .convert()?;
    ensure!(selected, "copy lost the retained native selection endpoint");
    let copied: String = wait::until(
        "complete source selection",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .execute("return window.__gtlCopiedSelection ?? null;", Vec::new())
                .await?
                .convert::<Option<String>>()?)
        },
    )
    .await?;
    let mut lines = copied.lines();
    ensure!(
        lines.next() == Some("// * src/large.rs, lines: 1..1000"),
        "unexpected copied context"
    );
    for number in 1..=1_000 {
        let expected =
            format!("pub const ROW_{number:05}: &str = \"single-file-benchmark-{number:05}\";");
        ensure!(
            lines.next() == Some(expected.as_str()),
            "copy omitted or changed source line {number}"
        );
    }
    ensure!(
        lines.next().is_none(),
        "copy included lines outside its selection"
    );
    Ok(())
}

async fn wait_for_marker(driver: &WebDriver, marker: &str) -> Result<()> {
    wait::until("requested logical source row", wait::ASSERTION_TIMEOUT, || async {
        let found: bool = driver.execute(
            "return [...document.querySelectorAll('[data-gtl-copy-text]')].some(line => line.textContent.includes(arguments[0]));",
            vec![serde_json::json!(marker)],
        ).await?.convert()?;
        Ok(found.then_some(()))
    }).await
}

async fn copy_whole_document(driver: &WebDriver) -> Result<()> {
    driver
        .execute(
            r"
        window.__gtlCopiedSelection = null;
        getSelection().selectAllChildren(document.querySelector('[data-gtl-diff-document]'));
        document.body.dispatchEvent(new ClipboardEvent('copy', {
            bubbles: true, cancelable: true, clipboardData: new DataTransfer(),
        }));
    ",
            Vec::new(),
        )
        .await?;
    let copied = wait::until(
        "whole logical document copied",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .execute("return window.__gtlCopiedSelection ?? null;", Vec::new())
                .await?
                .convert::<Option<String>>()?)
        },
    )
    .await?;
    ensure!(
        copied.lines().count() == 20_001
            && copied.starts_with("// * src/large.rs, lines: 1..20000\n")
            && copied.ends_with("single-file-benchmark-20000\";"),
        "whole-document copy omitted unmounted source"
    );
    assert_bounded_rows(driver).await
}

async fn assert_binary_row_ipc(driver: &WebDriver) -> Result<()> {
    let observation: serde_json::Value = driver.execute_async(r"
        const done = arguments[0];
        const [tab, range, selection, layout, density] = document.querySelector('[data-gtl-diff-document]')
            .getAttribute('data-view-identity').split(':');
        const invoke = window.__TAURI_INTERNALS__.invoke;
        (async () => {
            const streamId = await invoke('viewer_stream_rows_start', { request: {
                identity: { tab_id: Number(tab), range_generation: Number(range), selection_generation: Number(selection), render_options: { layout, density, wrap_lines: document.querySelector('[data-gtl-diff-document]').dataset.wrapLines === 'true' } },
                file: 'file-0', row_range: { start: 0, count: 1 },
            } });
            try {
                const batch = await invoke('viewer_stream_rows_next_batch', { streamId });
                done({ binary: batch instanceof ArrayBuffer, bytes: batch.byteLength ?? 0 });
            } finally {
                await invoke('viewer_stream_rows_cancel', { streamId });
            }
        })().catch(error => done({ error: String(error) }));
    ", Vec::new()).await?.convert()?;
    ensure!(
        observation["binary"] == true
            && observation["bytes"].as_u64().is_some_and(|bytes| bytes > 8),
        "row IPC fell back to JSON: {observation}"
    );
    Ok(())
}

async fn assert_bounded_rows(driver: &WebDriver) -> Result<()> {
    let counts: Vec<usize> = driver.execute(
        r"
        const root = document.querySelector('[data-gtl-diff-document]');
        return [Number(root.dataset.totalRows), root.querySelectorAll('[data-gtl-diff-row]').length];
        ", Vec::new(),
    ).await?.convert()?;
    ensure!(
        counts[0] == 20_005 && (1..=512).contains(&counts[1]),
        "unbounded or incomplete diff geometry: {counts:?}"
    );
    Ok(())
}

async fn rapid_scroll_cleans_up_delayed_starts(driver: &WebDriver) -> Result<()> {
    let observation: serde_json::Value = driver
        .execute_async(
            r"
        const done = arguments[0];
        const original = window.fetch;
        let starts = 0;
        let loading = false;
        const busy = () => document.querySelector('[role=tab][aria-selected=true]')?.getAttribute('aria-busy') === 'true';
        const observe = setInterval(() => { loading ||= busy(); }, 20);
        const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
        window.fetch = async function(input, options) {
            const response = await original.apply(this, arguments);
            if (String(input).includes('viewer_stream_rows_start')) {
                starts++;
                await sleep(400);
            }
            return response;
        };
        (async () => {
            try {
                const root = document.querySelector('[data-gtl-diff-document]');
                for (let index = 0; index < 30; index++) {
                    root.scrollTop = 10000 + index * 8000;
                    await sleep(32);
                }
                await sleep(3000);
                done({ starts, loading });
            } catch (error) { done({ error: String(error) }); }
            finally { window.fetch = original; clearInterval(observe); }
        })();
    ",
            Vec::new(),
        )
        .await?
        .convert()?;
    ensure!(
        observation["starts"]
            .as_u64()
            .is_some_and(|starts| starts >= 10),
        "rapid scroll did not exercise enough delayed starts: {observation}"
    );
    ensure!(
        observation["loading"] == true,
        "slow row retrieval never showed its loading state: {observation}"
    );
    wait::until("rapid scroll recovers without exhausted streams", wait::ASSERTION_TIMEOUT, || async {
        let usable: bool = driver.execute(r"
            const root = document.querySelector('[data-gtl-diff-document]');
            const box = root.getBoundingClientRect();
            const windows = [...root.querySelectorAll('[data-gtl-row-window]')].filter(element => {
                const row = element.getBoundingClientRect();
                return row.height > 0 && row.bottom > box.top && row.top < box.bottom;
            });
            return windows.length > 0 && windows.every(element => element.getAttribute('aria-busy') === 'false') &&
                document.querySelector('[role=tab][aria-selected=true]')?.getAttribute('aria-busy') === 'false' &&
                !root.innerText.includes('temporarily busy') && !root.innerText.includes('too many active streams');
        ", Vec::new()).await?.convert()?;
        Ok(usable.then_some(()))
    }).await?;
    assert_cached_scroll_stays_idle(driver).await?;
    assert_bounded_rows(driver).await
}

async fn assert_cached_scroll_stays_idle(driver: &WebDriver) -> Result<()> {
    let observation: serde_json::Value = driver.execute_async(r"
        const done = arguments[0];
        const original = window.fetch;
        const tab = document.querySelector('[role=tab][aria-selected=true]');
        let starts = 0;
        let loading = false;
        const observer = new MutationObserver(records => {
            loading ||= records.some(record => record.oldValue === 'true') || tab.getAttribute('aria-busy') === 'true';
        });
        observer.observe(tab, { attributes: true, attributeFilter: ['aria-busy'], attributeOldValue: true });
        window.fetch = function(input, options) {
            if (String(input).includes('viewer_stream_rows_start')) starts++;
            return original.apply(this, arguments);
        };
        document.querySelector('[data-gtl-diff-document]').scrollTop -= 1;
        setTimeout(() => {
            window.fetch = original;
            observer.disconnect();
            done({ starts, loading });
        }, 350);
    ", Vec::new()).await?.convert()?;
    ensure!(
        observation["starts"] == 0 && observation["loading"] == false,
        "cached scrolling started retrieval or flashed its spinner: {observation}"
    );
    Ok(())
}
