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

            let alpha_tab = tab_for(session.driver(), "one-shot-alpha").await?;
            alpha_tab.focus().await.context("focus alpha tab")?;
            alpha_tab
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

            let alpha_close = tab_close_for(session.driver(), "one-shot-alpha").await?;
            alpha_close.focus().await.context("focus alpha tab close")?;
            alpha_close
                .send_keys(Key::Enter)
                .await
                .context("close alpha tab with Enter")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;

            open_history(session.driver()).await?;
            session
                .driver()
                .refresh()
                .await
                .context("reload the packaged diff-history route")?;
            wait_for_history_heading(session.driver()).await?;
            wait_for_history_count(session.driver(), "one-shot-alpha", 1).await?;
            copy_history_json(session.driver(), "one-shot-alpha").await?;
            open_history_entry(session.driver(), "one-shot-alpha").await?;
            wait_for_ready_snapshot(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
                2,
            )
            .await?;

            tab_close_for(session.driver(), "one-shot-alpha")
                .await?
                .click()
                .await
                .context("close reopened alpha tab")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;
            tab_close_for(session.driver(), "one-shot-beta")
                .await?
                .click()
                .await
                .context("close final one-shot tab")?;
            wait_for_empty_viewer(session.driver()).await?;

            open_history(session.driver()).await?;
            open_history_entry(session.driver(), "one-shot-beta").await?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;

            session
                .driver()
                .refresh()
                .await
                .context("reload one-shot viewer")?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;
            assert_read_only_settings(session.driver()).await?;
            wait_for_ready_snapshot(session.driver(), "one-shot-beta", "beta-one-shot-marker", 1)
                .await?;

            fixture.seed_render_history(33)?;
            open_history(session.driver()).await?;
            support::selectors::by_accessible_name(session.driver(), "Last page")
                .await?
                .click()
                .await
                .context("open last render-history page")?;
            wait_for_last_history_page(session.driver()).await?;
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
const active = document.querySelector('[role="tab"][aria-selected="true"]');
const host = document.querySelector(
  '#viewer-diff-island[data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
const diff = host?.shadowRoot?.querySelector('[data-gtl-diff-document]');
return document.querySelectorAll('[role="tablist"][aria-label="Open diffs"]').length === 1
  && document.querySelectorAll('[role="tab"]').length === arguments[2]
  && active?.textContent.includes(arguments[0])
  && !active?.textContent.includes('Live')
  && document.querySelectorAll('#viewer-active-view').length === 1
  && diff?.textContent.includes(arguments[1]);
"#,
                    vec![
                        serde_json::json!(repository),
                        serde_json::json!(marker),
                        serde_json::json!(tab_count),
                    ],
                )
                .await
                .context("inspect one-shot Dioxus viewer state")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn tab_for(driver: &WebDriver, repository: &str) -> Result<WebElement> {
    tab_element(driver, repository, false, "tab activation control").await
}

async fn tab_close_for(driver: &WebDriver, repository: &str) -> Result<WebElement> {
    tab_element(driver, repository, true, "tab close control").await
}

async fn tab_element(
    driver: &WebDriver,
    repository: &str,
    close: bool,
    description: &str,
) -> Result<WebElement> {
    wait::until(
        &format!("{repository} {description}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const tab = Array.from(document.querySelectorAll('[role="tab"]'))
  .find((candidate) => candidate.textContent.includes(arguments[0]));
return arguments[1] ? tab?.parentElement?.querySelector('button[aria-label^="Close "]') ?? null : tab ?? null;
"#,
                    vec![serde_json::json!(repository), serde_json::json!(close)],
                )
                .await
                .with_context(|| format!("find {repository} {description}"))?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .with_context(|| format!("convert {repository} {description}"))
        },
    )
    .await
}

async fn open_history(driver: &WebDriver) -> Result<()> {
    support::selectors::by_accessible_name(driver, "History")
        .await?
        .click()
        .await
        .context("open diff history")?;
    wait_for_history_heading(driver).await
}

async fn wait_for_history_heading(driver: &WebDriver) -> Result<()> {
    support::selectors::by_css(driver, "#history-heading", "diff history heading")
        .await
        .map(|_| ())
}

async fn copy_history_json(driver: &WebDriver, repository: &str) -> Result<()> {
    history_action(driver, repository, "Copy ", " JSON")
        .await?
        .click()
        .await
        .with_context(|| format!("copy {repository} render JSON"))?;
    wait::until(
        &format!("{repository} render JSON copy confirmation"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const article = Array.from(document.querySelectorAll('[aria-label="Recent diff renders"] article'))
  .find((candidate) => candidate.textContent.includes(arguments[0]));
return article?.querySelector('button[aria-label^="Copy "][aria-label$=" JSON"]')?.title === 'Copied';
"#,
                    vec![serde_json::json!(repository)],
                )
                .await
                .context("inspect history copy confirmation")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn open_history_entry(driver: &WebDriver, repository: &str) -> Result<()> {
    history_action(driver, repository, "Open ", "")
        .await?
        .click()
        .await
        .with_context(|| format!("open {repository} from diff history"))
}

async fn history_action(
    driver: &WebDriver,
    repository: &str,
    label_prefix: &str,
    label_suffix: &str,
) -> Result<WebElement> {
    wait::until(
        &format!("{label_prefix}{repository} history action"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const article = Array.from(document.querySelectorAll('[aria-label="Recent diff renders"] article'))
  .find((candidate) => candidate.textContent.includes(arguments[0]));
return Array.from(article?.querySelectorAll('button') ?? []).find((button) => {
  const label = button.getAttribute('aria-label') ?? '';
  return label.startsWith(arguments[1]) && label.endsWith(arguments[2]);
}) ?? null;
"#,
                    vec![
                        serde_json::json!(repository),
                        serde_json::json!(label_prefix),
                        serde_json::json!(label_suffix),
                    ],
                )
                .await
                .with_context(|| format!("find {repository} history action"))?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .with_context(|| format!("convert {repository} history action"))
        },
    )
    .await
}

async fn wait_for_history_count(driver: &WebDriver, repository: &str, expected: u64) -> Result<()> {
    wait::until(
        &format!("{expected} history row(s) for {repository}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return Array.from(document.querySelectorAll('[aria-label="Recent diff renders"] article'))
  .filter((row) => row.textContent.includes(arguments[0])).length;
"#,
                    vec![serde_json::json!(repository)],
                )
                .await
                .context("count repository render-history rows")?;
            Ok((result.json().as_u64() == Some(expected)).then_some(()))
        },
    )
    .await
}

async fn wait_for_last_history_page(driver: &WebDriver) -> Result<()> {
    wait::until(
        "last render-history page",
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const output = document.querySelector('[aria-label^="Page "]');
const label = output?.getAttribute('aria-label') ?? '';
const match = /^Page (\d+) of (\d+)$/.exec(label);
return match !== null && Number(match[1]) === Number(match[2]) && Number(match[2]) > 1;
"#,
                    Vec::new(),
                )
                .await
                .context("inspect render-history page position")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn assert_read_only_settings(driver: &WebDriver) -> Result<()> {
    support::selectors::by_accessible_name(driver, "User settings")
        .await?
        .click()
        .await
        .context("open user settings")?;
    wait_for_read_only_settings(driver).await?;
    driver
        .refresh()
        .await
        .context("reload the packaged user-settings route")?;
    wait_for_read_only_settings(driver).await?;
    support::selectors::by_accessible_name(driver, "Viewer")
        .await?
        .click()
        .await
        .context("return to diff viewer")?;
    Ok(())
}

async fn wait_for_read_only_settings(driver: &WebDriver) -> Result<()> {
    wait::until(
        "read-only resolved viewer settings",
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const main = document.querySelector('main');
return document.querySelector('#settings-heading')?.textContent.trim() === 'User settings'
  && main?.textContent.includes('Edit the configuration file to change them.')
  && main?.querySelector('[aria-label="Resolved viewer settings"]') !== null
  && main?.querySelector('[aria-label="Project diff exclusions"]') !== null
  && main?.querySelector('input, select, textarea, button') === null;
"#,
                    Vec::new(),
                )
                .await
                .context("inspect read-only settings page")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await?;
    ensure!(
        driver.find_all(By::Css("#settings-heading")).await?.len() == 1,
        "settings page rendered duplicate headings"
    );
    Ok(())
}

async fn wait_for_empty_viewer(driver: &WebDriver) -> Result<()> {
    wait::until(
        "empty Dioxus viewer",
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return document.querySelectorAll('[role="tab"]').length === 0
  && document.querySelector('[role="tablist"][aria-label="Open diffs"]')?.textContent.includes('No open diffs')
  && document.querySelectorAll('#viewer-active-view[role="tabpanel"]').length === 1
  && document.querySelector('main')?.textContent.includes('No diff is open');
"#,
                    Vec::new(),
                )
                .await
                .context("inspect empty one-shot viewer")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}
