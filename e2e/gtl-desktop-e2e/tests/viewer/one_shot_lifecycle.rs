use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver, WebElement};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn viewer_one_shot_lifecycle() -> Result<()> {
    support::run_test("viewer-one-shot-lifecycle", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create(session.data_root())?;
            fixture.forward_alpha()?;
            wait_for_ready_snapshot(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
                1,
            )
            .await?;

            fixture.forward_beta()?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 2)
                .await?;

            let alpha_activate = tab_control(
                session.driver(),
                "one-shot-alpha",
                ".viewer-tab-activate",
                "alpha tab activation control",
            )
            .await?;
            alpha_activate.focus().await.context("focus alpha tab")?;
            alpha_activate
                .send_keys(Key::Enter)
                .await
                .context("activate alpha tab with Enter")?;
            wait_for_ready_snapshot(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
                2,
            )
            .await?;

            let alpha_close = tab_control(
                session.driver(),
                "one-shot-alpha",
                ".viewer-tab-close",
                "alpha tab close control",
            )
            .await?;
            alpha_close.focus().await.context("focus alpha tab close")?;
            alpha_close
                .send_keys(Key::Enter)
                .await
                .context("close alpha tab with Enter")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;

            open_history(session.driver()).await?;
            wait_for_history_count(session.driver(), "one-shot-alpha", 1).await?;
            history_row(session.driver(), "one-shot-alpha")
                .await?
                .click()
                .await
                .context("reopen alpha from render history")?;
            wait_for_ready_snapshot(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
                2,
            )
            .await?;
            close_history(session.driver()).await?;

            tab_control(
                session.driver(),
                "one-shot-alpha",
                ".viewer-tab-close",
                "reopened alpha tab close control",
            )
            .await?
            .click()
            .await
            .context("close reopened alpha tab")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;
            tab_control(
                session.driver(),
                "one-shot-beta",
                ".viewer-tab-close",
                "beta tab close control",
            )
            .await?
            .click()
            .await
            .context("close final one-shot tab")?;
            wait_for_empty_viewer(session.driver()).await?;

            let recovery =
                support::selectors::by_accessible_name(session.driver(), "Open history").await?;
            recovery
                .send_keys(Key::Enter)
                .await
                .context("open history recovery with Enter")?;
            history_row(session.driver(), "one-shot-beta")
                .await?
                .click()
                .await
                .context("recover beta from render history")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;

            session
                .driver()
                .refresh()
                .await
                .context("reload one-shot viewer")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;
            fixture.seed_render_history(33)?;
            open_history(session.driver()).await?;
            support::selectors::by_accessible_name(session.driver(), "Last page")
                .await?
                .click()
                .await
                .context("open last render-history page")?;
            wait_for_history_count(session.driver(), "one-shot-alpha", 1).await?;
            wait_for_history_count(session.driver(), "one-shot-beta", 1).await
        })
    })
    .await
}

async fn wait_for_ready_snapshot(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
    tab_count: u64,
) -> Result<()> {
    wait::until(
        &format!("{repository} ready with {tab_count} one-shot tab(s)"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const active = document.querySelector('.viewer-tab.active');
return document.querySelectorAll('#viewer-tabs').length === 1
  && document.querySelectorAll('#viewer-view').length === 1
  && document.querySelectorAll('.viewer-tab').length === arguments[2]
  && active?.querySelector('.viewer-tab-kind')?.textContent === 'S'
  && active?.querySelector('.viewer-tab-label')?.textContent.includes(arguments[0])
  && document.querySelector('#viewer-view')?.dataset.viewerState === 'ready'
  && document.querySelector('#viewer-view')?.textContent.includes(arguments[1]);
"#,
                    vec![
                        serde_json::json!(repository),
                        serde_json::json!(marker),
                        serde_json::json!(tab_count),
                    ],
                )
                .await
                .context("inspect one-shot viewer state")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn tab_control(
    driver: &WebDriver,
    repository: &str,
    selector: &str,
    description: &str,
) -> Result<WebElement> {
    wait::until(description, wait::ASSERTION_TIMEOUT, || async {
        let result = driver
            .execute(
                r#"
const repository = arguments[0];
const selector = arguments[1];
const tab = Array.from(document.querySelectorAll('.viewer-tab')).find((candidate) =>
  candidate.querySelector('.viewer-tab-label')?.textContent.includes(repository)
);
return tab?.querySelector(selector) ?? null;
"#,
                vec![serde_json::json!(repository), serde_json::json!(selector)],
            )
            .await
            .with_context(|| format!("find {description}"))?;
        if result.json().is_null() {
            return Ok(None);
        }
        result
            .element()
            .map(Some)
            .with_context(|| format!("convert {description}"))
    })
    .await
}

async fn open_history(driver: &WebDriver) -> Result<()> {
    support::selectors::by_css(driver, ".viewer-history-button", "render history button")
        .await?
        .click()
        .await
        .context("open render history")?;
    support::selectors::by_css(
        driver,
        "#viewer-history-popover:popover-open",
        "open render history popover",
    )
    .await
    .map(|_| ())
}

async fn close_history(driver: &WebDriver) -> Result<()> {
    if driver
        .find_all(By::Css("#viewer-history-popover:popover-open"))
        .await?
        .is_empty()
    {
        return Ok(());
    }
    support::selectors::by_accessible_name(driver, "Close history")
        .await?
        .click()
        .await
        .context("close render history")?;
    wait::until(
        "render history popover to close",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find_all(By::Css("#viewer-history-popover:popover-open"))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
    .await
}

async fn wait_for_history_count(driver: &WebDriver, repository: &str, expected: u64) -> Result<()> {
    wait::until(
        &format!("one history row for {repository}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return Array.from(document.querySelectorAll('#viewer-history .viewer-history-repo'))
  .filter((repo) => repo.textContent.trim() === arguments[0]).length;
"#,
                    vec![serde_json::json!(repository)],
                )
                .await
                .context("count repository render history rows")?;
            Ok((result.json().as_u64() == Some(expected)).then_some(()))
        },
    )
    .await
}

async fn history_row(driver: &WebDriver, repository: &str) -> Result<WebElement> {
    wait::until(
        &format!("render history row for {repository}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return Array.from(document.querySelectorAll('#viewer-history .viewer-history-row')).find((row) =>
  row.querySelector('.viewer-history-repo')?.textContent.trim() === arguments[0]
) ?? null;
"#,
                    vec![serde_json::json!(repository)],
                )
                .await
                .context("find repository render history row")?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .context("convert repository render history row")
        },
    )
    .await
}

async fn wait_for_empty_viewer(driver: &WebDriver) -> Result<()> {
    wait::until(
        "empty viewer with history recovery",
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return document.querySelectorAll('.viewer-tab').length === 0
  && document.querySelector('.viewer-status-empty') !== null
  && document.activeElement?.classList.contains('viewer-recovery-button');
"#,
                    Vec::new(),
                )
                .await
                .context("inspect empty one-shot viewer")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await?;
    ensure!(
        driver.find_all(By::Css("#viewer-tabs")).await?.len() == 1,
        "empty viewer rendered duplicate tab regions"
    );
    Ok(())
}
