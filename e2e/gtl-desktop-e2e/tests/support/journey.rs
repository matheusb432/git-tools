use std::process::Command;

use anyhow::{Context, Result, ensure};
use thirtyfour::{By, Key, WebDriver, WebElement};

use super::{
    fixture::{EditorRecord, ViewerFixture},
    selectors::{by_accessible_name, by_accessible_name_within, by_css},
    session::TestSession,
    wait::{self, ASSERTION_TIMEOUT},
};

const COMPLETE_DIFF_SELECTOR: &str = "[data-gtl-diff-document][data-view-state='complete'][data-chunks-complete='true'][aria-busy='false']";
const DELETE_DESCRIPTION: &str =
    "This removes the saved live view and closes its tab. Render history remains available.";

pub async fn assert_forwarded_live_view(
    session: &TestSession,
    expected_content: &str,
) -> Result<()> {
    let driver = session.driver();
    wait_for_active_live_view(driver, "live-view", expected_content, 1).await?;
    ensure!(
        driver.find_all(By::Css("iframe")).await?.is_empty(),
        "viewer rendered an iframe instead of the client-rendered diff document"
    );
    Ok(())
}

pub async fn assert_mobile_navigation(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .set_window_rect(0, 0, 390, 844)
        .await
        .context("resize viewer to the mobile viewport")?;

    by_accessible_name(driver, "Files")
        .await?
        .click()
        .await
        .context("open changed files")?;
    by_css(
        driver,
        "#mobile-files-panel[open]",
        "open changed-files dialog",
    )
    .await?;
    wait::until(
        "work.txt in the mobile changed-files dialog",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r"
const dialog = document.querySelector('#mobile-files-panel[open]');
return dialog?.textContent.includes('Changed files') && dialog.textContent.includes('work.txt');
",
            )
            .await?
            .then_some(()))
        },
    )
    .await?;
    by_accessible_name_within(driver, "#mobile-files-panel[open]", "Close Changed files")
        .await?
        .click()
        .await
        .context("close changed files")?;
    wait_for_dialog_closed(driver, "mobile-files-panel").await?;

    driver
        .execute(
            "if (document.activeElement instanceof HTMLElement) document.activeElement.blur();",
            Vec::new(),
        )
        .await
        .context("normalize focus before mobile evidence capture")?;
    Ok(())
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
            r"
const rows = document.querySelectorAll('[data-gtl-diff-document] [data-gtl-diff-row]');
return { count: rows.length, firstHeight: rows[0]?.offsetHeight ?? 0 };
",
            Vec::new(),
        )
        .await
        .context("measure first client-rendered diff row")?;
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

pub async fn assert_default_navigation_reachable(session: &TestSession) -> Result<()> {
    wait::until(
        "display, changed-files, and commit navigation at the default window width",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                session.driver(),
                r"
const displayed = (element) => element !== null
  && getComputedStyle(element).display !== 'none'
  && getComputedStyle(element).visibility !== 'hidden'
  && element.getClientRects().length > 0;
return window.innerWidth === 1200
  && ['mobile-display-trigger', 'mobile-files-trigger', 'mobile-commits-trigger']
    .every((id) => displayed(document.getElementById(id)));
",
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

pub async fn assert_chunked_live_view(
    session: &TestSession,
    fixture: &ViewerFixture,
) -> Result<u64> {
    fixture.forward_sized_live_view("live-chunked", "chunked-live-marker", 600)?;
    let driver = session.driver();
    wait_for_active_live_view(driver, "live-chunked", "chunked-live-marker", 2).await?;
    wait::until(
        "multiple diff line pages to render in the active document",
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r"
const documentElement = document.querySelector(arguments[0]);
return {
  hasDocument: documentElement !== null,
  hasTarget: documentElement?.querySelector('#viewer-diff-0') !== null,
  rows: documentElement?.querySelectorAll('[data-gtl-diff-row]').length ?? 0
};
",
                    vec![serde_json::json!(COMPLETE_DIFF_SELECTOR)],
                )
                .await
                .context("inspect paged diff document")?;
            let observation = result.json();
            let row_count = observation["rows"].as_u64().unwrap_or_default();
            let ready = observation["hasDocument"].as_bool() == Some(true)
                && observation["hasTarget"].as_bool() == Some(true)
                && row_count > 256;
            Ok(ready.then_some(row_count))
        },
    )
    .await
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
        "second live view ready without duplicate Dioxus regions",
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const active = document.querySelector('[role="tab"][aria-selected="true"]');
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return {
  tablists: document.querySelectorAll('[role="tablist"][aria-label="Open diffs"]').length,
  activeViews: document.querySelectorAll('#viewer-active-view').length,
  tabs: document.querySelectorAll('[role="tab"]').length,
  latestReady: active?.textContent.includes('live-latest')
    && active.textContent.includes('Live')
    && diff?.textContent.includes('second-live-marker')
};
"#,
                    Vec::new(),
                )
                .await
                .context("inspect overlapping live-update Dioxus regions")?;
            let observation = result.json();
            let latest_ready = observation["latestReady"].as_bool().unwrap_or(false);
            let duplicate = observation["tablists"]
                .as_u64()
                .is_some_and(|count| count > 1)
                || observation["activeViews"]
                    .as_u64()
                    .is_some_and(|count| count > 1);
            Ok((duplicate || latest_ready).then_some(observation.clone()))
        },
    )
    .await?;

    ensure!(
        observation["tablists"].as_u64() == Some(1),
        "overlapping live updates rendered {} tablists",
        observation["tablists"]
    );
    ensure!(
        observation["activeViews"].as_u64() == Some(1),
        "overlapping live updates rendered {} active-view regions",
        observation["activeViews"]
    );
    ensure!(
        observation["tabs"].as_u64() == Some(4),
        "overlapping live updates rendered {} tabs instead of four",
        observation["tabs"]
    );
    ensure!(
        observation["latestReady"].as_bool() == Some(true),
        "the latest live view did not become the active completed diff"
    );
    Ok(())
}

pub async fn delete_temporary_live_views(
    session: &TestSession,
    chunked_row_count: u64,
) -> Result<()> {
    let driver = session.driver();
    wait_for_active_live_view(driver, "live-latest", "second-live-marker", 4).await?;
    delete_active_live_view(driver, "live-latest").await?;

    wait_for_active_live_document(driver, "live-heavy", 3).await?;
    delete_active_live_view(driver, "live-heavy").await?;

    wait_for_active_live_view(driver, "live-chunked", "chunked-live-marker", 2).await?;
    assert_diff_excludes_with_row_count(driver, "first-live-marker", chunked_row_count).await?;
    delete_active_live_view(driver, "live-chunked").await?;

    wait_for_active_live_view(driver, "live-view", "alpha-v2", 1).await?;
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
        || async {
            Ok(viewer_uses_split_layout(session.driver())
                .await?
                .then_some(()))
        },
    )
    .await
}

pub async fn refresh_and_assert_unavailable(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    by_accessible_name(driver, "Refresh diff")
        .await?
        .click()
        .await
        .context("refresh unavailable live view")?;
    wait::until(
        "missing live repository to render its typed unavailable state",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r#"
const alert = document.querySelector('main [role="alert"]');
const active = document.querySelector('[role="tab"][aria-selected="true"]');
const text = alert?.textContent ?? '';
return active?.textContent.includes('Live')
  && text.includes('Render stopped (DirNotFound)')
  && text.includes('was not found')
  && !text.includes('Render failed')
  && document.querySelector('[data-gtl-diff-document]') === null;
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
return document.querySelectorAll('[role="tab"]').length === 0
  && document.querySelector('[role="tablist"][aria-label="Open diffs"]')?.textContent.includes('No open diffs')
  && document.querySelector('main')?.textContent.includes('No diff is open')
  && !Array.from(document.querySelectorAll('button')).some((button) =>
    button.getAttribute('aria-label') === 'Delete live view'
  );
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
    by_accessible_name(driver, "Display")
        .await?
        .click()
        .await
        .context("open default-width display controls")?;
    by_css(
        driver,
        "#mobile-display-panel[open]",
        "open default-width display-controls dialog",
    )
    .await?;
    by_accessible_name_within(driver, "#mobile-display-panel[open]", "Side by side")
        .await?
        .click()
        .await
        .context("select split layout from default-width display controls")?;
    wait::until("completed split diff layout", ASSERTION_TIMEOUT, || async {
        Ok(script_bool(
            driver,
            r#"
const split = Array.from(document.querySelectorAll('button'))
  .find((button) => button.textContent.trim() === 'Side by side' && button.offsetParent !== null);
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return split?.getAttribute('aria-pressed') === 'true'
  && diff?.dataset.layout === 'split'
  && diff.querySelector('[data-layout="split"]') !== null;
"#,
        )
        .await?
        .then_some(()))
    })
    .await?;
    by_accessible_name_within(
        driver,
        "#mobile-display-panel[open]",
        "Close Display controls",
    )
    .await?
    .click()
    .await
    .context("close default-width display controls")?;
    wait_for_dialog_closed(driver, "mobile-display-panel").await
}

pub async fn refresh_and_assert_alpha_v2(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    let previous_identity = by_css(driver, COMPLETE_DIFF_SELECTOR, "completed diff document")
        .await?
        .attr("data-view-identity")
        .await
        .context("read pre-refresh diff identity")?
        .context("completed diff document is missing its view identity")?;
    by_accessible_name(driver, "Refresh diff")
        .await?
        .click()
        .await
        .context("refresh live view")?;
    wait::until(
        "refreshed live view rendering alpha-v2 with a new range generation",
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return diff?.dataset.viewIdentity !== arguments[0] && diff.textContent.includes('alpha-v2');
"#,
                    vec![serde_json::json!(previous_identity)],
                )
                .await
                .context("inspect refreshed diff identity and content")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

pub async fn select_commit_patch_and_restore_range(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .set_window_rect(0, 0, 1440, 900)
        .await
        .context("resize viewer to expose the desktop commit shelf")?;
    let range_identity = by_css(
        driver,
        COMPLETE_DIFF_SELECTOR,
        "completed range diff document",
    )
    .await?
    .attr("data-view-identity")
    .await
    .context("read range diff identity")?
    .context("completed range diff document is missing its view identity")?;
    let commit = commit_button(driver, "live view v2").await?;
    commit
        .click()
        .await
        .context("select live view v2 commit patch")?;

    wait::until(
        "standalone live view v2 commit patch",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool_with_string(
                driver,
                r#"
const shelf = document.querySelector('aside[aria-label="Commits"]');
const selected = shelf?.querySelector('button[aria-pressed="true"]');
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
const text = diff?.textContent ?? '';
const file = diff?.querySelector('details[data-gtl-diff-file]');
return selected?.textContent.includes('live view v2')
  && selected.disabled === false
  && diff?.dataset.viewIdentity !== arguments[0]
  && text.includes('alpha-v1')
  && text.includes('alpha-v2')
  && file?.querySelector('[data-lines-added="1"]') !== null
  && file?.querySelector('[data-lines-removed="1"]') !== null;
"#,
                &range_identity,
            )
            .await?
            .then_some(()))
        },
    )
    .await?;
    let selected_identity = by_css(
        driver,
        COMPLETE_DIFF_SELECTOR,
        "completed selected-commit diff document",
    )
    .await?
    .attr("data-view-identity")
    .await
    .context("read selected-commit diff identity")?
    .context("selected-commit diff document is missing its view identity")?;

    by_accessible_name(driver, "Range")
        .await?
        .click()
        .await
        .context("restore the complete live view range")?;
    wait::until(
        "restored complete range after commit patch",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool_with_string(
                driver,
                r#"
const shelf = document.querySelector('aside[aria-label="Commits"]');
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
const text = diff?.textContent ?? '';
const file = diff?.querySelector('details[data-gtl-diff-file]');
return shelf?.querySelector('button[aria-pressed="true"]') === null
  && diff?.dataset.viewIdentity !== arguments[0]
  && text.includes('alpha-v2')
  && !text.includes('alpha-v1')
  && file?.querySelector('[data-lines-added="1"]') !== null
  && file?.querySelector('[data-lines-removed="0"]') !== null;
"#,
                &selected_identity,
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

pub async fn delete_and_assert_empty_state(session: &TestSession) -> Result<()> {
    let driver = session.driver();
    open_compact_delete_dialog(driver).await?;
    wait_for_focused_element(driver, "#delete-live-view-dialog button", "Cancel").await?;
    driver
        .active_element()
        .await
        .context("read initially focused deletion control")?
        .send_keys(Key::Escape)
        .await
        .context("cancel live-view deletion with Escape")?;
    wait_for_dialog_closed(driver, "delete-live-view-dialog").await?;
    wait_for_focused_element(driver, "#mobile-display-trigger", "Display").await?;

    open_compact_delete_dialog(driver).await?;
    wait_for_focused_element(driver, "#delete-live-view-dialog button", "Cancel").await?;
    let confirm = wait_for_delete_confirmation(driver).await?;
    confirm
        .focus()
        .await
        .context("focus deletion confirmation")?;
    confirm
        .send_keys(Key::Enter)
        .await
        .context("confirm live-view deletion with Enter")?;
    wait_for_empty_viewer(driver).await
}

async fn open_compact_delete_dialog(driver: &WebDriver) -> Result<()> {
    by_accessible_name(driver, "Display")
        .await?
        .click()
        .await
        .context("open compact display controls")?;
    by_css(
        driver,
        "#mobile-display-panel[open]",
        "open compact display-controls dialog",
    )
    .await?;
    let delete =
        by_accessible_name_within(driver, "#mobile-display-panel[open]", "Delete live view")
            .await?;
    delete
        .focus()
        .await
        .context("focus compact Delete live view")?;
    delete
        .send_keys(Key::Enter)
        .await
        .context("activate compact Delete live view with Enter")?;
    wait_for_delete_dialog(driver).await.map(|_| ())
}

async fn confirm_live_view_deletion(driver: &WebDriver) -> Result<()> {
    wait_for_delete_confirmation(driver)
        .await?
        .click()
        .await
        .context("confirm live-view deletion")
}

async fn delete_active_live_view(driver: &WebDriver, repository: &str) -> Result<()> {
    open_compact_delete_dialog(driver)
        .await
        .with_context(|| format!("open {repository} deletion confirmation"))?;
    confirm_live_view_deletion(driver)
        .await
        .with_context(|| format!("delete temporary {repository} live view"))
}

async fn wait_for_delete_confirmation(driver: &WebDriver) -> Result<WebElement> {
    wait_for_delete_dialog(driver).await?;
    by_accessible_name_within(driver, "#delete-live-view-dialog[open]", "Delete live view").await
}

async fn wait_for_delete_dialog(driver: &WebDriver) -> Result<WebElement> {
    wait::until(
        "live-view deletion alert dialog",
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const dialog = document.querySelector('#delete-live-view-dialog[open][role="alertdialog"]');
return dialog?.textContent.includes(arguments[0]) ? dialog : null;
"#,
                    vec![serde_json::json!(DELETE_DESCRIPTION)],
                )
                .await
                .context("inspect live-view deletion dialog")?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .context("convert live-view deletion dialog")
        },
    )
    .await
}

async fn wait_for_active_live_view(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
    tab_count: u64,
) -> Result<()> {
    wait::until(
        &format!("{repository} live view to be active with a completed diff"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const active = document.querySelector('[role="tab"][aria-selected="true"]');
const documentElement = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return document.querySelectorAll('[role="tablist"][aria-label="Open diffs"]').length === 1
  && document.querySelectorAll('[role="tab"]').length === arguments[2]
  && active?.textContent.includes(arguments[0])
  && active.textContent.includes('Live')
  && documentElement?.textContent.includes(arguments[1]);
"#,
                    vec![
                        serde_json::json!(repository),
                        serde_json::json!(marker),
                        serde_json::json!(tab_count),
                    ],
                )
                .await
                .context("inspect active Dioxus live view")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn wait_for_active_live_document(
    driver: &WebDriver,
    repository: &str,
    tab_count: u64,
) -> Result<()> {
    wait::until(
        &format!("{repository} live diff document to be active"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
const active = document.querySelector('[role="tab"][aria-selected="true"]');
const diff = document.querySelector('[data-gtl-diff-document]');
return document.querySelectorAll('[role="tab"]').length === arguments[1]
  && active?.textContent.includes(arguments[0])
  && active.textContent.includes('Live')
  && document.querySelectorAll('#viewer-active-view').length === 1
  && ['streaming', 'complete'].includes(diff?.dataset.viewState);
"#,
                    vec![serde_json::json!(repository), serde_json::json!(tab_count)],
                )
                .await
                .context("inspect active Dioxus live document")?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn assert_diff_excludes_with_row_count(
    driver: &WebDriver,
    unexpected: &str,
    expected_rows: u64,
) -> Result<()> {
    let result = driver
        .execute(
            r#"
const documentElement = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return {
  containsUnexpected: documentElement?.textContent.includes(arguments[0]) ?? false,
  rows: documentElement?.querySelectorAll('[data-gtl-diff-row]').length ?? 0
};
"#,
            vec![serde_json::json!(unexpected)],
        )
        .await
        .context("inspect replacement diff for stale rows")?;
    let observation = result.json();
    ensure!(
        observation["containsUnexpected"].as_bool() == Some(false),
        "replacement diff contains stale content {unexpected:?}"
    );
    let rows = observation["rows"].as_u64().unwrap_or_default();
    ensure!(
        rows == expected_rows,
        "replacement diff has {rows} rows instead of the original {expected_rows} paged-fixture rows"
    );
    Ok(())
}

async fn wait_for_empty_viewer(driver: &WebDriver) -> Result<()> {
    wait::until(
        "empty viewer after deleting live view",
        ASSERTION_TIMEOUT,
        || async {
            Ok(script_bool(
                driver,
                r#"
return document.querySelectorAll('[role="tab"]').length === 0
  && document.querySelector('[role="tablist"][aria-label="Open diffs"]')?.textContent.includes('No open diffs')
  && document.querySelector('main')?.textContent.includes('No diff is open')
  && document.querySelector('[data-gtl-diff-document]') === null
  && !document.querySelector('#delete-live-view-dialog[open]');
"#,
            )
            .await?
            .then_some(()))
        },
    )
    .await
}

async fn wait_for_dialog_closed(driver: &WebDriver, id: &str) -> Result<()> {
    let selector = format!("#{id}[open]");
    wait::until(&format!("{id} to close"), ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Css(&selector))
            .await?
            .is_empty()
            .then_some(()))
    })
    .await
}

async fn wait_for_focused_element(driver: &WebDriver, selector: &str, text: &str) -> Result<()> {
    wait::until(
        &format!("{text:?} to receive focus"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r"
const active = document.activeElement;
return active?.matches(arguments[0]) && active.textContent.trim() === arguments[1];
",
                    vec![serde_json::json!(selector), serde_json::json!(text)],
                )
                .await
                .with_context(|| format!("inspect focus for {text:?}"))?;
            Ok(result.json().as_bool().unwrap_or(false).then_some(()))
        },
    )
    .await
}

async fn commit_button(driver: &WebDriver, subject: &str) -> Result<WebElement> {
    wait::until(
        &format!("commit action for {subject:?}"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    r#"
return Array.from(document.querySelectorAll('aside[aria-label="Commits"] button'))
  .find((button) => button.offsetParent !== null && button.textContent.includes(arguments[0])) ?? null;
"#,
                    vec![serde_json::json!(subject)],
                )
                .await
                .with_context(|| format!("find commit action for {subject:?}"))?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .with_context(|| format!("convert commit action for {subject:?}"))
        },
    )
    .await
}

async fn viewer_uses_split_layout(driver: &WebDriver) -> Result<bool> {
    script_bool(
        driver,
        r#"
const diff = document.querySelector(
  '[data-gtl-diff-document][data-view-state="complete"][data-chunks-complete="true"][aria-busy="false"]'
);
return document.querySelectorAll('[role="tab"]').length === 1
  && document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.includes('Live')
  && diff?.dataset.layout === 'split'
  && diff.querySelector('[data-layout="split"]') !== null;
"#,
    )
    .await
}

async fn script_bool(driver: &WebDriver, script: &str) -> Result<bool> {
    let result = driver.execute(script, Vec::new()).await?;
    Ok(result.json().as_bool().unwrap_or(false))
}

async fn script_bool_with_string(driver: &WebDriver, script: &str, value: &str) -> Result<bool> {
    let result = driver
        .execute(script, vec![serde_json::json!(value)])
        .await?;
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
