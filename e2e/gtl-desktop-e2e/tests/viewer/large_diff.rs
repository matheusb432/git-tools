use anyhow::{Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::{
    desktop_scroll_baseline::{create_single_file_repository, forward_single_file_fixture},
    support::{self, wait},
};

#[tokio::test(flavor = "multi_thread")]
async fn huge_file_search_and_selection_cross_unmounted_rows() -> Result<()> {
    support::run_test("large-diff", |session| Box::pin(run(session))).await
}

async fn run(session: &mut support::session::TestSession) -> Result<()> {
    let repository = create_single_file_repository(session.data_root().join("large-diff"))?;
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
    support::visible(driver, By::Id("viewer-diff-find-input"))
        .await?
        .send_keys("single-file-benchmark-20000")
        .await?;
    wait_for_marker(driver, "single-file-benchmark-20000").await?;
    assert_bounded_rows(driver).await
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
