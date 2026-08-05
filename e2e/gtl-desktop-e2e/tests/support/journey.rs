use std::process::Command;

use anyhow::{Context, Result, ensure};
use thirtyfour::{By, Key, WebDriver, error::WebDriverErrorInner};

use super::{
    fixture::{EditorRecord, ViewerFixture},
    selectors::{by_accessible_name, by_css},
    session::TestSession,
    wait::{self, ASSERTION_TIMEOUT},
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
    wait::until(
        &format!("forwarded live view to render {expected_content}"),
        ASSERTION_TIMEOUT,
        || async {
            let view = by_css(driver, "#viewer-view", "viewer view").await?;
            Ok((view.text().await?.contains(expected_content)).then_some(()))
        },
    )
    .await?;
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
  latestReady: document.querySelector('#viewer-view')?.dataset.viewerState === 'ready'
    && (document.querySelector('#viewer-view')?.textContent.includes('second-live-marker') ?? false)
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

pub async fn delete_temporary_live_views(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    for (current_repository, current_marker, next_repository, next_marker, remaining_tabs) in [
        (
            "live-latest",
            "second-live-marker",
            "live-heavy",
            "first-live-marker",
            2_u64,
        ),
        (
            "live-heavy",
            "first-live-marker",
            "live-view",
            "alpha-v2",
            1,
        ),
    ] {
        wait_for_active_live_view(
            driver,
            current_repository,
            current_marker,
            remaining_tabs + 1,
        )
        .await?;
        by_accessible_name(driver, "Delete saved live view")
            .await?
            .click()
            .await
            .with_context(|| format!("delete temporary {current_repository} live view"))?;
        accept_delete_confirmation(driver).await?;
        wait_for_htmx_idle(driver, &format!("deleting {current_repository} live view")).await?;
        wait_for_active_live_view(driver, next_repository, next_marker, remaining_tabs).await?;
    }
    Ok(())
}

pub async fn assert_restarted_live_view(
    session: &TestSession,
    expected_content: &str,
) -> Result<()> {
    assert_forwarded_live_view(session, expected_content).await?;
    wait::until(
        "restarted live view with persisted split layout",
        ASSERTION_TIMEOUT,
        || async { Ok(viewer_state(session.driver()).await?.then_some(())) },
    )
    .await
}

pub async fn refresh_and_assert_unavailable(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    by_accessible_name(driver, "Refresh")
        .await?
        .click()
        .await
        .context("refresh unavailable live view")?;
    wait::until(
        "live view to publish Unavailable instead of Render failed",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r#"
const view = document.querySelector('#viewer-view');
const active = document.querySelector('.viewer-tab.active');
return view?.dataset.viewerState === 'broken'
  && view.querySelector('.viewer-status-broken') !== null
  && view.textContent.includes('Unavailable')
  && !view.textContent.includes('Render failed')
  && active?.querySelector('.viewer-tab-state[aria-label="Unavailable"]') !== null
  && active.querySelector('.viewer-tab-state[aria-label="Render failed"]') === null;
"#,
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

pub async fn assert_durable_empty_state(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    wait::until(
        "deleted live view to remain absent after viewer restart",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r#"
return document.querySelector('.viewer-status-empty') !== null
  && !Array.from(document.querySelectorAll('.viewer-tab-kind')).some((kind) => kind.textContent === 'L')
  && !Array.from(document.querySelectorAll('button')).some((button) => button.textContent.trim() === 'Delete live view');
"#,
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

pub async fn select_split_layout(session: &TestSession) -> Result<()> {
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

    Ok(())
}

pub async fn refresh_and_assert_alpha_v2(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    by_accessible_name(driver, "Refresh")
        .await?
        .click()
        .await
        .context("refresh live view")?;
    wait_for_htmx_idle(driver, "refreshing the live view").await?;
    wait::until(
        "refreshed live view rendering alpha-v2",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                "return document.querySelector('#viewer-view')?.textContent.includes('alpha-v2') ?? false;",
            )
            .await?
            .then_some(()))
        },
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

    driver
        .execute(
            r##"
const card = document.querySelector("#viewer-view .cline.active .commit-select");
if (!(card instanceof HTMLButtonElement)) {
  throw new Error("selected commit clear action is missing");
}
card.click();
"##,
            Vec::new(),
        )
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

pub async fn delete_and_assert_empty_state(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    let delete = by_accessible_name(driver, "Delete saved live view").await?;
    delete.focus().await.context("focus Delete live view")?;
    delete
        .send_keys(Key::Enter)
        .await
        .context("activate Delete live view with Enter")?;
    accept_delete_confirmation(driver).await?;
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

    Ok(())
}

async fn accept_delete_confirmation(driver: &WebDriver) -> Result<()> {
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
    Ok(())
}

async fn wait_for_live_tab(driver: &WebDriver, description: &str) -> Result<()> {
    wait::until(description, ASSERTION_TIMEOUT, || async {
        Ok(live_tab_is_only_active_tab(driver).await?.then_some(()))
    })
    .await
}

async fn wait_for_active_live_view(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
    tab_count: u64,
) -> Result<()> {
    wait::until(
        &format!("{repository} live view to be active and ready"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const active = document.querySelector('.viewer-tab.active');
const view = document.querySelector('#viewer-view');
return document.querySelectorAll('.viewer-tab').length === arguments[2]
  && active?.querySelector('.viewer-tab-label')?.textContent.includes(arguments[0])
  && view?.dataset.viewerState === 'ready'
  && view.textContent.includes(arguments[1]);
"#,
                    vec![
                        serde_json::json!(repository),
                        serde_json::json!(marker),
                        serde_json::json!(tab_count),
                    ],
                )
                .await
                .context("inspect active live view")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn wait_for_htmx_idle(driver: &WebDriver, label: &str) -> Result<()> {
    wait::until(
        &format!("HTMX to settle after {label}"),
        ASSERTION_TIMEOUT,
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
