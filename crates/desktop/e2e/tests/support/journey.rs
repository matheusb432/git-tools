use std::{future::Future, process::Command, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use thirtyfour::{By, Key, WebDriver, error::WebDriverErrorInner};
use tokio::time::{Instant, sleep, timeout};

use super::{
    fixture::{EditorRecord, ViewerFixture},
    refresh_delivery::{
        RefreshDelivery, RefreshDeliveryAction, RefreshDeliveryObservation, refresh_delivery_window,
    },
    selectors::{by_accessible_name, by_css},
    session::TestSession,
    wait::{self, ASSERTION_TIMEOUT, WEBDRIVER_OPERATION_TIMEOUT},
};

const DELETE_CONFIRMATION: &str = "Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live.";

pub async fn assert_forwarded_live_view(
    session: &TestSession,
    expected_content: &str,
) -> Result<()> {
    let driver = session.driver();
    wait_for_live_tab(driver, "forwarded live view").await?;
    wait_for_htmx_idle(driver, "forwarding live view").await?;
    let layout = by_css(driver, "#viewer-view .layout", "rendered viewer layout").await?;
    ensure!(layout.is_displayed().await?, "viewer layout is hidden");
    ensure!(
        driver
            .find_all(By::Css("#viewer-view iframe"))
            .await?
            .is_empty(),
        "viewer rendered an iframe instead of the server-rendered layout"
    );
    ensure!(
        by_css(driver, "#viewer-view", "viewer view")
            .await?
            .text()
            .await?
            .contains(expected_content),
        "forwarded live view did not render {expected_content}"
    );
    Ok(())
}

pub async fn assert_mobile_navigation(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .set_window_rect(0, 0, 390, 844)
        .await
        .context("resize viewer to the mobile viewport")?;

    by_accessible_name(driver, "Changed files")
        .await?
        .click()
        .await
        .context("open changed files")?;
    by_css(
        driver,
        "#viewer-files-popover:popover-open [data-file-target]",
        "changed file in the open mobile popover",
    )
    .await?;
    let changed_file_text = driver
        .execute(
            "return document.querySelector('#viewer-files-popover:popover-open [data-file-target]')?.textContent ?? '';",
            Vec::new(),
        )
        .await
        .context("read changed file text")?
        .json()
        .as_str()
        .unwrap_or_default()
        .to_string();
    ensure!(
        changed_file_text.contains("work.txt"),
        "mobile changed-files popover omitted work.txt"
    );
    by_accessible_name(driver, "Close changed files")
        .await?
        .click()
        .await
        .context("close changed files")?;
    assert_popover_closed(driver, "viewer-files-popover").await?;

    by_accessible_name(driver, "Commits in range")
        .await?
        .click()
        .await
        .context("open commits in range")?;
    by_css(
        driver,
        "#viewer-commits-popover:popover-open .cline .sub",
        "commit in the open mobile popover",
    )
    .await?;
    let commit_text = driver
        .execute(
            "return document.querySelector('#viewer-commits-popover:popover-open .cline .sub')?.textContent ?? '';",
            Vec::new(),
        )
        .await
        .context("read commit subject")?
        .json()
        .as_str()
        .unwrap_or_default()
        .to_string();
    ensure!(
        commit_text.contains("live view v2"),
        "mobile commits popover omitted live view v2"
    );
    by_accessible_name(driver, "Close commits in range")
        .await?
        .click()
        .await
        .context("close commits in range")?;
    assert_popover_closed(driver, "viewer-commits-popover").await?;

    by_accessible_name(driver, "View settings")
        .await?
        .click()
        .await
        .context("open mobile view settings")?;
    by_css(
        driver,
        "#viewer-controls-popover:popover-open",
        "open mobile view settings",
    )
    .await?;
    by_accessible_name(driver, "Close view settings")
        .await?
        .click()
        .await
        .context("close mobile view settings")?;
    assert_popover_closed(driver, "viewer-controls-popover").await?;

    let metrics = driver
        .execute(
            r"
const main = document.querySelector('#viewer-view .main');
const summary = main?.querySelector('details.file > summary');
if (!(main instanceof HTMLElement) || !(summary instanceof HTMLElement)) {
  return null;
}
const style = getComputedStyle(main);
return {
  paddingLeft: Number.parseFloat(style.paddingLeft),
  paddingRight: Number.parseFloat(style.paddingRight),
  stickyGap: summary.getBoundingClientRect().top - main.getBoundingClientRect().top
};
",
            Vec::new(),
        )
        .await
        .context("measure mobile diff spacing")?;
    let metrics = metrics.json();
    ensure!(
        !metrics.is_null(),
        "mobile diff spacing targets are missing"
    );
    let padding_left = metrics["paddingLeft"].as_f64().unwrap_or(f64::INFINITY);
    let padding_right = metrics["paddingRight"].as_f64().unwrap_or(f64::INFINITY);
    let sticky_gap = metrics["stickyGap"].as_f64().unwrap_or(f64::INFINITY);
    ensure!(
        padding_left <= 4.0 && padding_right <= 4.0,
        "mobile diff padding is {padding_left}px left and {padding_right}px right"
    );
    ensure!(
        sticky_gap.abs() <= 1.0,
        "sticky file header starts {sticky_gap}px below the diff viewport"
    );
    driver
        .execute(
            "if (document.activeElement instanceof HTMLElement) document.activeElement.blur();",
            Vec::new(),
        )
        .await
        .context("normalize focus before mobile evidence capture")?;
    Ok(())
}

async fn assert_popover_closed(driver: &WebDriver, id: &str) -> Result<()> {
    let selector = format!("#{id}:popover-open");
    wait::until(&format!("{id} to close"), ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Css(&selector))
            .await?
            .is_empty()
            .then_some(()))
    })
    .await
}

pub async fn assert_configured_editor_launch(
    session: &TestSession,
    fixture: &ViewerFixture,
) -> Result<()> {
    let driver = session.driver();
    by_accessible_name(driver, "Open in IDE")
        .await?
        .click()
        .await
        .context("open changed file in configured editor")?;
    wait_for_htmx_idle(driver, "opening the diff file").await?;
    let editor = wait::until(
        "configured editor launch record",
        ASSERTION_TIMEOUT,
        || async {
            if !fixture.editor_record_exists() {
                return Ok(None);
            }
            fixture.editor_record().map(Some)
        },
    )
    .await?;
    assert_editor_launch(fixture, &editor)?;
    fixture.release_editor(editor.pid)?;
    wait::until(
        "configured editor recorder exit",
        ASSERTION_TIMEOUT,
        || async {
            Ok((fixture.editor_exit_exists() && !process_is_alive(editor.pid)).then_some(()))
        },
    )
    .await
}

pub async fn assert_first_paint(session: &TestSession) -> Result<()> {
    let script_result = session
        .driver()
        .execute(
            "const rows = document.querySelectorAll('#viewer-view .filebody .dl'); return { count: rows.length, firstHeight: rows[0]?.offsetHeight ?? 0 };",
            Vec::new(),
        )
        .await
        .context("measure first rendered diff row")?;
    let rows = script_result.json();
    let count = rows["count"].as_u64().unwrap_or_default();
    let first_height = rows["firstHeight"].as_u64().unwrap_or_default();
    ensure!(count > 0, "viewer rendered no diff rows on first paint");
    ensure!(
        first_height > 0,
        "viewer first diff row has zero height on first paint"
    );
    Ok(())
}

pub async fn assert_overlapping_live_updates(
    session: &TestSession,
    fixture: &ViewerFixture,
) -> Result<()> {
    let driver = session.driver();
    driver
        .set_window_rect(0, 0, 1280, 800)
        .await
        .context("resize viewer for overlapping live updates")?;
    fixture.forward_sized_live_view("live-heavy", "first-live-marker", 45_000)?;
    fixture.forward_sized_live_view("live-latest", "second-live-marker", 1)?;

    let observation = wait::until(
        "second live view ready without duplicate viewer regions",
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r"
return {
  tabs: document.querySelectorAll('#viewer-tabs').length,
  views: document.querySelectorAll('#viewer-view').length,
  latestReady: document.querySelector('#viewer-view')?.textContent.includes('second-live-marker') ?? false
};
",
                    Vec::new(),
                )
                .await
                .context("inspect overlapping live-update viewer regions")?;
            let observation = result.json();
            let tabs = observation["tabs"].as_u64().unwrap_or_default();
            let views = observation["views"].as_u64().unwrap_or_default();
            let latest_ready = observation["latestReady"].as_bool().unwrap_or(false);
            Ok(((tabs != 1 || views != 1) || latest_ready).then_some(observation.clone()))
        },
    )
    .await?;

    ensure!(
        observation["tabs"].as_u64() == Some(1),
        "overlapping live updates rendered {} tab regions",
        observation["tabs"]
    );
    ensure!(
        observation["views"].as_u64() == Some(1),
        "overlapping live updates rendered {} view regions",
        observation["views"]
    );
    ensure!(
        observation["latestReady"].as_bool() == Some(true),
        "the latest live view did not become the active ready view"
    );
    Ok(())
}

pub async fn select_and_restore_split_layout(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .execute(
            "const input = document.querySelector(\"input[name='viewer-layout'][value='split']\"); if (!(input instanceof HTMLInputElement)) { throw new Error('split layout input is missing'); } input.click();",
            Vec::new(),
        )
        .await
        .context("select split layout")?;
    wait_for_htmx_idle(driver, "selecting the split layout").await?;
    by_css(driver, "#viewer-view .diff-split", "split diff layout").await?;
    ensure!(
        by_css(
            driver,
            "input[name='viewer-layout'][value='split']",
            "selected split layout control",
        )
        .await?
        .is_selected()
        .await?,
        "split layout control is not selected"
    );

    driver.refresh().await.context("reload viewer session")?;
    wait::until(
        "restored live view and split layout",
        ASSERTION_TIMEOUT,
        || async { Ok(viewer_state(driver).await?.then_some(())) },
    )
    .await
}

pub async fn refresh_and_assert_alpha_v2(session: &TestSession) -> Result<()> {
    let deadline = Instant::now() + ASSERTION_TIMEOUT;
    wait::within(
        "refresh live view and render alpha-v2",
        ASSERTION_TIMEOUT,
        refresh_and_assert_alpha_v2_within(session, deadline),
    )
    .await
}

pub async fn select_commit_patch_and_restore_range(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .execute(
            r##"
const card = Array.from(document.querySelectorAll("#viewer-view .commit-select"))
  .find((button) => button.textContent.includes("live view v2"));
if (!(card instanceof HTMLButtonElement)) {
  throw new Error("live view v2 commit action is missing");
}
card.click();
"##,
            Vec::new(),
        )
        .await
        .context("select live view v2 commit patch")?;
    wait::until(
        "standalone live view v2 commit patch",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r##"
const view = document.querySelector("#viewer-view");
const text = view?.textContent ?? "";
const file = view?.querySelector("details.file");
return view?.dataset.viewerState === "ready"
  && view.querySelector(".cline.active .commit-select")?.textContent.includes("live view v2")
  && Array.from(view.querySelectorAll("button")).some((button) => button.textContent.trim() === "Show all changes")
  && text.includes("alpha-v1")
  && text.includes("alpha-v2")
  && text.includes("1 commit")
  && file?.querySelector(".a")?.textContent.trim() === "+1"
  && file?.querySelector(".d")?.textContent.trim() === "−1";
"##,
            )
            .await?
            .then_some(()))
        },
    )
    .await?;

    by_accessible_name(driver, "Show all changes")
        .await?
        .click()
        .await
        .context("restore the complete live view range")?;
    wait_for_htmx_idle(driver, "restoring the complete range").await?;
    wait::until(
        "restored complete range after commit patch",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r##"
const view = document.querySelector("#viewer-view");
const text = view?.textContent ?? "";
const file = view?.querySelector("details.file");
return view?.dataset.viewerState === "ready"
  && view.querySelector(".cline.active") === null
  && !Array.from(view.querySelectorAll("button")).some((button) => button.textContent.trim() === "Show all changes")
  && !text.includes("alpha-v1")
  && text.includes("alpha-v2")
  && file?.querySelector(".a")?.textContent.trim() === "+1"
  && file?.querySelector(".d")?.textContent.trim() === "−0";
"##,
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

async fn refresh_and_assert_alpha_v2_within(
    session: &TestSession,
    deadline: Instant,
) -> Result<()> {
    let driver = session.driver();
    install_refresh_delivery_observer(driver, deadline).await?;
    let attempts = refresh_until_request_starts(driver, deadline).await?;
    wait_for_htmx_idle_until(
        driver,
        "refreshing the live view",
        refresh_budget_remaining(deadline)?,
    )
    .await?;
    wait::until(
        "refreshed live view rendering alpha-v2",
        refresh_budget_remaining(deadline)?,
        || async {
            Ok(script_bool(
                driver,
                "return document.querySelector('#viewer-view')?.textContent.includes('alpha-v2') ?? false;",
            )
            .await?
            .then_some(()))
        },
    )
    .await?;
    let observation = refresh_delivery_observation(driver, deadline).await?;
    ensure!(
        observation.click_deliveries == 1
            && observation.request_starts == 1
            && observation.duplicate_clicks_blocked == 0,
        "Refresh delivery was not exactly once after {attempts} WebDriver click attempt(s): {} delivered click(s), {} request start(s), {} duplicate click(s) blocked",
        observation.click_deliveries,
        observation.request_starts,
        observation.duplicate_clicks_blocked
    );
    Ok(())
}

pub async fn delete_and_restore_empty_state(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    let delete = by_accessible_name(driver, "Delete saved live view").await?;
    delete.focus().await.context("focus Delete live view")?;
    delete
        .send_keys(Key::Enter)
        .await
        .context("activate Delete live view with Enter")?;
    let confirmation = wait::until(
        "Delete live view confirmation",
        ASSERTION_TIMEOUT,
        || async {
            match driver.get_alert_text().await {
                Ok(text) => Ok(Some(text)),
                Err(error) if matches!(error.as_inner(), WebDriverErrorInner::NoSuchAlert(..)) => {
                    Ok(None)
                }
                Err(error) => Err(error.into()),
            }
        },
    )
    .await?;
    ensure!(
        confirmation == DELETE_CONFIRMATION,
        "unexpected Delete live view confirmation: {confirmation:?}"
    );
    driver
        .accept_alert()
        .await
        .context("accept Delete live view confirmation")?;
    wait_for_htmx_idle(driver, "deleting the live view").await?;
    wait::until(
        "focused empty state after deleting live view",
        ASSERTION_TIMEOUT,
        || async { Ok(empty_state_is_focused(driver).await?.then_some(())) },
    )
    .await?;
    by_css(driver, ".viewer-status-empty", "empty viewer state").await?;
    by_css(
        driver,
        ".viewer-recovery-button",
        "focused viewer recovery control",
    )
    .await?;

    driver
        .refresh()
        .await
        .context("reload empty viewer state")?;
    by_css(
        driver,
        ".viewer-status-empty",
        "restored empty viewer state",
    )
    .await?;
    ensure!(
        !has_button(driver, "Delete live view").await?,
        "Delete live view remains available after empty state reload"
    );
    Ok(())
}

async fn wait_for_live_tab(driver: &WebDriver, description: &str) -> Result<()> {
    wait::until(description, ASSERTION_TIMEOUT, || async {
        Ok(live_tab_is_only_active_tab(driver).await?.then_some(()))
    })
    .await
}

async fn wait_for_htmx_idle(driver: &WebDriver, label: &str) -> Result<()> {
    wait_for_htmx_idle_until(driver, label, ASSERTION_TIMEOUT).await
}

async fn wait_for_htmx_idle_until(
    driver: &WebDriver,
    label: &str,
    wait_timeout: Duration,
) -> Result<()> {
    wait::until(
        &format!("HTMX to settle after {label}"),
        wait_timeout,
        || async {
            Ok(script_bool(
                driver,
                "return !document.querySelector('.hx-request, .htmx-request');",
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

async fn install_refresh_delivery_observer(driver: &WebDriver, deadline: Instant) -> Result<()> {
    let result =
        within_refresh_webdriver_operation(deadline, "install Refresh delivery observer", async {
            driver
                .execute(
                    r#"
const buttons = Array.from(
  document.querySelectorAll(".viewer-controls button[hx-get$='/refresh']")
);
if (buttons.length !== 1) {
  return buttons.length;
}
const delivery = {
  button: buttons[0],
  clickDeliveries: 0,
  requestStarts: 0,
  duplicateClicksBlocked: 0
};
window.__gtlRefreshDelivery = delivery;
const semanticRefreshButton = (event) => event.composedPath().find((element) =>
  element instanceof HTMLButtonElement &&
  element.matches(".viewer-controls button[hx-get$='/refresh']")
);
document.addEventListener('click', (event) => {
  const button = semanticRefreshButton(event);
  if (!button) {
    return;
  }
  delivery.clickDeliveries += 1;
  if (delivery.clickDeliveries === 1) {
    delivery.button = button;
    return;
  }
  delivery.duplicateClicksBlocked += 1;
  event.preventDefault();
  event.stopImmediatePropagation();
}, true);
document.addEventListener('htmx:beforeRequest', (event) => {
  if (event.detail?.elt === delivery.button) {
    delivery.requestStarts += 1;
  }
});
return buttons.length;
"#,
                    Vec::new(),
                )
                .await
                .context("install Refresh delivery observer")
        })
        .await?;
    ensure!(
        result.json().as_u64() == Some(1),
        "expected one Refresh button when installing the delivery observer"
    );
    Ok(())
}

async fn refresh_until_request_starts(driver: &WebDriver, deadline: Instant) -> Result<usize> {
    let mut delivery = RefreshDelivery::default();
    let mut observation = refresh_delivery_observation(driver, deadline).await?;
    let mut delivery_window_elapsed = true;

    loop {
        match delivery.next_action(
            observation,
            delivery_window_elapsed,
            Instant::now() < deadline,
        ) {
            RefreshDeliveryAction::Click(attempt) => {
                click_refresh(driver, deadline, attempt).await?;
                (observation, delivery_window_elapsed) =
                    observe_refresh_delivery_window(driver, deadline).await?;
            }
            RefreshDeliveryAction::AwaitDelivery => {
                (observation, delivery_window_elapsed) =
                    observe_refresh_delivery_window(driver, deadline).await?;
            }
            RefreshDeliveryAction::AwaitRequest => {
                observation = wait_for_refresh_request_start(driver, deadline).await?;
                delivery_window_elapsed = false;
            }
            RefreshDeliveryAction::Complete(attempts) => return Ok(attempts),
            RefreshDeliveryAction::Exhausted(attempts) => {
                bail!(
                    "Refresh click was not delivered after {attempts} WebDriver click attempt(s)"
                );
            }
            RefreshDeliveryAction::DeadlineExceeded(attempts) => {
                bail!(
                    "Refresh did not render alpha-v2 within {ASSERTION_TIMEOUT:?} after {attempts} WebDriver click attempt(s)"
                );
            }
            RefreshDeliveryAction::Duplicate {
                click_deliveries,
                request_starts,
                duplicate_clicks_blocked,
            } => {
                bail!(
                    "Refresh duplicate click was blocked before HTMX: {click_deliveries} delivered click(s), {request_starts} request start(s), {duplicate_clicks_blocked} duplicate click(s) blocked"
                );
            }
        }
    }
}

async fn click_refresh(driver: &WebDriver, deadline: Instant, attempt: usize) -> Result<()> {
    within_refresh_webdriver_operation(
        deadline,
        &format!("deliver Refresh click attempt {attempt}"),
        async {
            by_css(
                driver,
                ".viewer-controls button[hx-get$='/refresh']",
                "desktop Refresh action",
            )
            .await?
            .click()
            .await
            .with_context(|| format!("refresh live view attempt {attempt}"))
        },
    )
    .await
}

async fn observe_refresh_delivery_window(
    driver: &WebDriver,
    deadline: Instant,
) -> Result<(RefreshDeliveryObservation, bool)> {
    let delivery_window = refresh_delivery_window(refresh_budget_remaining(deadline)?);
    match timeout(delivery_window, async {
        loop {
            let observation = refresh_delivery_observation(driver, deadline).await?;
            if observation.delivery_started() {
                return Ok::<_, anyhow::Error>(observation);
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    {
        Ok(result) => Ok((result?, false)),
        Err(_) => Ok((refresh_delivery_observation(driver, deadline).await?, true)),
    }
}

async fn wait_for_refresh_request_start(
    driver: &WebDriver,
    deadline: Instant,
) -> Result<RefreshDeliveryObservation> {
    wait::until(
        "Refresh HTMX request to start after its click was delivered",
        refresh_budget_remaining(deadline)?,
        || async {
            let observation = refresh_delivery_observation(driver, deadline).await?;
            Ok(
                (observation.request_starts > 0 || observation.click_deliveries > 1)
                    .then_some(observation),
            )
        },
    )
    .await
}

async fn refresh_delivery_observation(
    driver: &WebDriver,
    deadline: Instant,
) -> Result<RefreshDeliveryObservation> {
    let result = within_refresh_webdriver_operation(
        deadline,
        "read Refresh delivery observation",
        async {
            driver
                .execute(
                    "return { clickDeliveries: window.__gtlRefreshDelivery?.clickDeliveries ?? 0, requestStarts: window.__gtlRefreshDelivery?.requestStarts ?? 0, duplicateClicksBlocked: window.__gtlRefreshDelivery?.duplicateClicksBlocked ?? 0 };",
                    Vec::new(),
                )
                .await
                .context("read Refresh delivery observation")
        },
    )
    .await?;
    let observation = result.json();
    Ok(RefreshDeliveryObservation::new(
        observation["clickDeliveries"].as_u64().unwrap_or_default(),
        observation["requestStarts"].as_u64().unwrap_or_default(),
        observation["duplicateClicksBlocked"]
            .as_u64()
            .unwrap_or_default(),
    ))
}

async fn within_refresh_webdriver_operation<T, F>(
    deadline: Instant,
    description: &str,
    operation: F,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    wait::within(
        description,
        WEBDRIVER_OPERATION_TIMEOUT.min(refresh_budget_remaining(deadline)?),
        operation,
    )
    .await
}

fn refresh_budget_remaining(deadline: Instant) -> Result<Duration> {
    let budget_remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(
        !budget_remaining.is_zero(),
        "Refresh did not render alpha-v2 within {ASSERTION_TIMEOUT:?}"
    );
    Ok(budget_remaining)
}

async fn viewer_state(driver: &WebDriver) -> Result<bool> {
    script_bool(
        driver,
        "return document.querySelectorAll('.viewer-tab').length === 1 && document.querySelector('.viewer-tab.active .viewer-tab-kind')?.textContent === 'L' && document.querySelector('#viewer-view .diff-split') !== null && document.querySelector(\"input[name='viewer-layout'][value='split']\")?.checked === true;",
    )
    .await
}

async fn live_tab_is_only_active_tab(driver: &WebDriver) -> Result<bool> {
    script_bool(
        driver,
        "return document.querySelectorAll('.viewer-tab').length === 1 && document.querySelector('.viewer-tab.active .viewer-tab-kind')?.textContent === 'L';",
    )
    .await
}

async fn empty_state_is_focused(driver: &WebDriver) -> Result<bool> {
    script_bool(
        driver,
        "return !Array.from(document.querySelectorAll('.viewer-tab-kind')).some((kind) => kind.textContent === 'L') && document.querySelector('.viewer-status-empty') !== null && document.activeElement?.classList.contains('viewer-recovery-button');",
    )
    .await
}

async fn has_button(driver: &WebDriver, expected: &str) -> Result<bool> {
    let result = driver
        .execute(
            "return Array.from(document.querySelectorAll('button')).some((button) => button.textContent.trim() === arguments[0]);",
            vec![serde_json::json!(expected)],
        )
        .await
        .context("inspect viewer buttons")?;
    Ok(result.json().as_bool().unwrap_or(false))
}

async fn script_bool(driver: &WebDriver, script: &str) -> Result<bool> {
    let result = driver.execute(script, Vec::new()).await?;
    Ok(result.json().as_bool().unwrap_or(false))
}

fn assert_editor_launch(fixture: &ViewerFixture, editor: &EditorRecord) -> Result<()> {
    ensure!(
        editor.pid > 0,
        "editor recorder reported an invalid process id"
    );
    ensure!(
        process_is_alive(editor.pid),
        "editor recorder {} is not alive after launch",
        editor.pid
    );
    ensure!(
        !fixture.editor_exit_exists(),
        "editor recorder exited before the release signal"
    );
    let repository = fixture.canonical_repository()?;
    let expected_arguments = vec![
        "--profile".to_owned(),
        "Viewer E2E".to_owned(),
        "--reuse-window".to_owned(),
        repository.to_string_lossy().into_owned(),
        "--goto".to_owned(),
        repository.join("work.txt").to_string_lossy().into_owned(),
    ];
    ensure!(
        editor.working_directory == repository.to_string_lossy(),
        "configured editor working directory differs from the repository"
    );
    ensure!(
        editor.arguments == expected_arguments,
        "configured editor arguments differ: {:?}",
        editor.arguments
    );
    Ok(())
}

#[cfg(unix)]
fn process_is_alive(process_id: u32) -> bool {
    Command::new("kill")
        .args(["-0", &process_id.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_is_alive(process_id: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {process_id}"), "/NH"])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&process_id.to_string())
        })
}
