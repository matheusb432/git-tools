use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use thirtyfour::{
    By, WebDriver, WebElement,
    prelude::{ElementQueryable as _, ElementWaitable as _},
    stringmatch::StringMatch,
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_opens_project_comparisons_and_restores_them() -> Result<()> {
    support::run_test("viewer-projects", |session| Box::pin(run_projects(session))).await
}

async fn run_projects(session: &mut support::session::TestSession) -> Result<()> {
    let fixture_home = std::env::var_os("HOME").context("fixture home")?;
    let fixture_root = tempfile::Builder::new()
        .prefix(".gtl-projects-")
        .tempdir_in(fixture_home)?;
    let fixture = support::fixture::ProjectsFixture::create(fixture_root.path())?;
    session.catalogue.set_projects(&[
        ("ALP", "Alpha", &fixture.alpha),
        ("BET", "Beta", &fixture.beta),
        ("INI", "Initial", &fixture.initial),
        ("MIS", "Missing", &fixture.missing),
    ])?;
    home(session.driver()).await?;
    session.driver().refresh().await?;
    support::wait::until(
        "four project cards",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok((session
                .driver()
                .find_all(By::Css("[data-project-card]"))
                .await?
                .len()
                == 4)
                .then_some(()))
        },
    )
    .await?;
    wait_for_project_statuses(session.driver()).await?;
    verify_watched_status(session.driver(), &fixture.beta).await?;
    let alpha = card(session.driver(), "Alpha").await?;
    let alpha_text = alpha.text().await?;
    ensure!(
        alpha_text.contains("[!?]"),
        "project card omitted CLI working-tree symbols: {alpha_text:?}"
    );
    let beta_text = card(session.driver(), "Beta").await?.text().await?;
    ensure!(
        beta_text.contains("[✓]"),
        "clean project omitted CLI symbol: {beta_text:?}"
    );
    ensure!(
        alpha_text.contains("Unpushed commits"),
        "commit count must name its unit: {alpha_text:?}"
    );
    ensure!(
        !alpha_text.contains("Modified") && !alpha_text.contains("Untracked"),
        "project card retained path count labels: {alpha_text:?}"
    );
    ensure!(
        session
            .driver()
            .find_all(By::Css("input[type='search']"))
            .await?
            .is_empty(),
        "overview must not add search"
    );
    for name in ["Beta", "Initial", "Missing"] {
        ensure!(
            card(session.driver(), name)
                .await?
                .find_all(By::Css("[aria-label='Create snapshot']"))
                .await?
                .is_empty(),
            "project {name} offered an empty snapshot"
        );
    }
    ensure!(
        card(session.driver(), "Initial")
            .await?
            .text()
            .await?
            .contains("Local comparison branch 'main' is missing"),
        "missing comparison branch must be explained"
    );
    ensure!(
        !action(session.driver(), "Missing", "Open live")
            .await?
            .is_enabled()
            .await?,
        "missing repository action must be disabled"
    );
    support::evidence::capture(session.driver(), "projects-desktop", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(session.driver(), "projects-narrow", true).await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;

    exercise_table(session).await?;
    review_comparisons(session, &fixture).await?;
    exercise_local_comparison(session, &fixture).await?;
    restore_comparisons(session).await
}

async fn exercise_local_comparison(
    session: &support::session::TestSession,
    fixture: &support::fixture::ProjectsFixture,
) -> Result<()> {
    fixture.use_local_comparison()?;
    home(session.driver()).await?;
    let alpha = card(session.driver(), "Alpha").await?;
    alpha
        .find(By::Css("button[aria-label^='Comparison branch']"))
        .await?
        .click()
        .await?;
    let input = alpha.find(By::Css("input")).await?;
    input.clear().await?;
    input.send_keys("review-base").await?;
    alpha
        .find(By::Css("button[type='submit']"))
        .await?
        .click()
        .await?;
    card(session.driver(), "Alpha")
        .await?
        .query(By::Css(
            "button[aria-label='Comparison branch: review-base']",
        ))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .context("stored local comparison branch")?;
    support::evidence::capture(session.driver(), "project-comparison-settings", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(session.driver(), "project-comparison-settings-narrow", true)
        .await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;
    action(session.driver(), "Alpha", "Open live")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    support::wait::until(
        "local comparison base in the titlebar",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            for header in session.driver().find_all(By::Css("header")).await? {
                if header.is_displayed().await?
                    && header
                        .prop("textContent")
                        .await?
                        .is_some_and(|text| text.contains("refs/heads/review-base"))
                {
                    return Ok(Some(()));
                }
            }
            Ok(None)
        },
    )
    .await?;
    let content = session
        .driver()
        .query(By::Css("main"))
        .and_displayed()
        .first()
        .await?
        .text()
        .await?;
    ensure!(
        !content.contains("refreshed-project-marker"),
        "branch comparison included uncommitted work"
    );
    fixture.restore_upstream()?;
    Ok(())
}

async fn toggle_modified(driver: &WebDriver) -> Result<()> {
    driver
        .query(By::Css("button[aria-label='Modified files']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    Ok(())
}

async fn review_comparisons(
    session: &support::session::TestSession,
    fixture: &support::fixture::ProjectsFixture,
) -> Result<()> {
    action(session.driver(), "Alpha", "Open live")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    let url = session.driver().current_url().await?;
    toggle_modified(session.driver()).await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "untracked-project-marker",
    )
    .await?;
    ensure!(
        session
            .driver()
            .query(By::Css("main"))
            .and_displayed()
            .first()
            .await?
            .text()
            .await?
            .contains("local-project-marker"),
        "working tree omitted tracked work"
    );
    ensure!(
        session.driver().current_url().await? == url,
        "modified files opened another tab"
    );
    fixture.change_untracked()?;
    toggle_modified(session.driver()).await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    toggle_modified(session.driver()).await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "refreshed-project-marker",
    )
    .await?;
    toggle_modified(session.driver()).await?;
    home(session.driver()).await?;
    action(session.driver(), "Alpha", "Create snapshot")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(session.driver(), "Alpha", "committed-project-marker").await?;
    ensure!(
        !session
            .driver()
            .query(By::Css("main"))
            .and_displayed()
            .first()
            .await?
            .text()
            .await?
            .contains("refreshed-project-marker"),
        "snapshot included working tree changes"
    );
    pin_active_snapshot(session.driver()).await?;
    home(session.driver()).await?;
    review_snapshot_dialog(session).await?;
    action(session.driver(), "Beta", "Open live")
        .await?
        .click()
        .await?;
    session
        .driver()
        .query(By::Css("main"))
        .ignore_errors(true)
        .and_displayed()
        .with_text(StringMatch::new("No changes").partial())
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    home(session.driver()).await?;
    action(session.driver(), "Initial", "Open live")
        .await?
        .click()
        .await?;
    toggle_modified(session.driver()).await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-initial",
        "initial-project-marker",
    )
    .await?;
    Ok(())
}

async fn review_snapshot_dialog(session: &support::session::TestSession) -> Result<()> {
    session.catalogue.seed_project_snapshots("ALP", 65)?;
    session
        .driver()
        .find(By::Css("button[aria-label='Snapshots for Alpha']"))
        .await?
        .click()
        .await?;
    let dialog = session
        .driver()
        .query(By::Css("dialog[open]"))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        dialog
            .find(By::Css("select"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("project:Alpha"),
        "project snapshots filter was not preselected"
    );
    wait_for_snapshot_rows(session.driver(), 30).await?;
    let aligned = session.driver().execute(r"
        const dialog = document.querySelector('dialog[open]');
        const label = dialog.querySelector('label');
        const select = dialog.querySelector('select');
        const a = label.getBoundingClientRect();
        const b = select.getBoundingClientRect();
        const footer = dialog.querySelector('footer').getBoundingClientRect();
        return Math.abs(a.y + a.height / 2 - b.y - b.height / 2) < 1 && footer.bottom <= dialog.getBoundingClientRect().bottom;
    ", vec![]).await?.convert::<bool>()?;
    ensure!(
        aligned,
        "snapshot filter is uneven or pagination is outside the dialog"
    );
    support::evidence::capture(session.driver(), "project-snapshots-dialog", true).await?;
    dialog
        .find(By::Css("[aria-label='Last page']"))
        .await?
        .click()
        .await?;
    wait_for_snapshot_rows(session.driver(), 5).await?;
    dialog
        .find(By::Css("[aria-label='Previous page']"))
        .await?
        .click()
        .await?;
    wait_for_snapshot_rows(session.driver(), 30).await?;
    dialog
        .find(By::Css("[aria-label='First page']"))
        .await?
        .click()
        .await?;
    dialog
        .query(By::Css("output[aria-label='Page 1 of 3']"))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(session.driver(), "project-snapshots-narrow", true).await?;
    dialog.send_keys(thirtyfour::Key::Escape).await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;

    Ok(())
}

async fn wait_for_snapshot_rows(driver: &WebDriver, expected: usize) -> Result<()> {
    support::wait::until(
        "snapshot table page",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok((driver
                .find_all(By::Css("dialog[open] tbody tr"))
                .await?
                .len()
                == expected)
                .then_some(()))
        },
    )
    .await
}

async fn restore_comparisons(session: &mut support::session::TestSession) -> Result<()> {
    home(session.driver()).await?;
    action(session.driver(), "Alpha", "Open live")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("[role='tab'][title*='projects-alpha']"))
            .await?
            .len()
            == 1,
        "reopening created a duplicate live comparison tab"
    );
    session.restart().await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;
    session
        .driver()
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    session
        .driver()
        .query(By::Css(
            "[role='tab'][title*='projects-alpha'][title*='Unpushed commits']",
        ))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    close_others_preserves_pins(session).await
}

async fn close_others_preserves_pins(session: &support::session::TestSession) -> Result<()> {
    let pinned = session
        .driver()
        .query(By::Css("button[aria-label^='Unpin ']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        pinned.is_displayed().await?,
        "pinned snapshot did not survive restart"
    );
    let active = session
        .driver()
        .find(By::Css("[role='tab'][aria-selected='true']"))
        .await?;
    session
        .driver()
        .action_chain()
        .context_click_element(&active)
        .perform()
        .await?;
    session
        .driver()
        .query(By::Css("[role='menu']:popover-open [role='menuitem']"))
        .with_text(StringMatch::new("Close others").partial())
        .first()
        .await?
        .click()
        .await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("button[aria-label^='Unpin ']"))
            .await?
            .len()
            == 1,
        "close others removed the pin"
    );
    pinned.click().await?;
    support::wait::until(
        "snapshot unpinned",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok(session
                .driver()
                .find_all(By::Css("button[aria-label^='Unpin ']"))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
    .await?;
    Ok(())
}

async fn pin_active_snapshot(driver: &WebDriver) -> Result<()> {
    let active = driver
        .find(By::Css("[role='tab'][aria-selected='true']"))
        .await?;
    driver
        .action_chain()
        .context_click_element(&active)
        .perform()
        .await?;
    support::evidence::capture(driver, "tab-context-menu", true).await?;
    driver
        .query(By::Css("[role='menu']:popover-open [role='menuitem']"))
        .with_text(StringMatch::new("Pin tab").partial())
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    driver
        .query(By::Css("button[aria-label^='Unpin ']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    support::evidence::capture(driver, "pinned-snapshot", true).await?;
    Ok(())
}

async fn home(driver: &WebDriver) -> Result<()> {
    driver
        .query(By::Css("a[aria-label='Projects']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    driver
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        driver.current_url().await?.path() == "/projects",
        "Projects must own its route"
    );
    ensure!(
        driver
            .find_all(By::Css("[role='tab'][aria-selected='true']"))
            .await?
            .is_empty(),
        "Projects must not select a diff tab"
    );
    Ok(())
}

async fn card(driver: &WebDriver, name: &str) -> Result<WebElement> {
    driver
        .query(By::Css(format!(
            "article[aria-label='{name}'][aria-busy='false']"
        )))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .map_err(Into::into)
}

async fn action(driver: &WebDriver, project: &str, label: &str) -> Result<WebElement> {
    card(driver, project)
        .await?
        .query(By::Css(format!(
            "a[aria-label='{label}'], button[aria-label='{label}']"
        )))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .map_err(Into::into)
}

#[tokio::test(flavor = "multi_thread")]
async fn catalogue_failure_keeps_cli_diffs_usable() -> Result<()> {
    support::run_test("viewer-projects-offline", |session| {
        Box::pin(async move {
            session.catalogue.make_unavailable()?;
            session.driver().refresh().await?;
            session
                .driver()
                .query(By::Css("[role='alert']"))
                .with_text(StringMatch::new("project catalogue is unavailable").partial())
                .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
                .first()
                .await?;
            let fixture = support::fixture::ViewerFixture::create_named(
                session.data_root(),
                "offline-live-view",
            )?;
            fixture.forward_live_view()?;
            support::wait_for_active_diff(session.driver(), "offline-live-view", "alpha-v1")
                .await?;
            let mut launch_url = session.driver().current_url().await?;
            launch_url.set_path("/diffs");
            session.driver().goto(launch_url.as_str()).await?;
            support::wait_for_active_diff(session.driver(), "offline-live-view", "alpha-v1")
                .await?;
            launch_url.set_path("/");
            session.driver().goto(launch_url.as_str()).await?;
            session
                .driver()
                .query(By::Id("projects-heading"))
                .ignore_errors(true)
                .and_displayed()
                .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
                .first()
                .await?;
            Ok(())
        })
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_poll_every_thirty_seconds_only_on_the_active_route() -> Result<()> {
    support::run_test("viewer-projects-polling", |session| {
        Box::pin(run_polling(session))
    })
    .await
}

async fn run_polling(session: &mut support::session::TestSession) -> Result<()> {
    support::wait::until(
        "initial Projects request",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok((session.project_requests()? > 0).then_some(())) },
    )
    .await?;
    let requests = session.project_requests()?;
    let started = tokio::time::Instant::now();
    support::wait::until(
        "scheduled Projects refresh",
        Duration::from_secs(35),
        || async { Ok((session.project_requests()? > requests).then_some(())) },
    )
    .await?;
    ensure!(
        started.elapsed() >= Duration::from_secs(25),
        "Projects refreshed before the 30-second interval"
    );
    let fixture =
        support::fixture::ViewerFixture::create_named(session.data_root(), "polling-live-view")?;
    fixture.forward_live_view()?;
    support::wait_for_active_diff(session.driver(), "polling-live-view", "alpha-v1").await?;
    let requests = session.project_requests()?;
    tokio::time::sleep(Duration::from_secs(31)).await;
    ensure!(
        session.project_requests()? == requests,
        "Projects kept polling on a diff route"
    );
    home(session.driver()).await?;
    support::wait::until(
        "Projects refresh on route entry",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok((session.project_requests()? > requests).then_some(())) },
    )
    .await?;
    session.driver().back().await?;
    support::wait_for_active_diff(session.driver(), "polling-live-view", "alpha-v1").await?;
    session.driver().forward().await?;
    session
        .driver()
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("[role='tab'][aria-selected='true']"))
            .await?
            .is_empty(),
        "Forward to Projects selected a diff"
    );
    Ok(())
}

async fn select_view(driver: &WebDriver, label: &str) -> Result<()> {
    driver
        .query(By::Css(format!("button[aria-label='{label}']")))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    driver
        .query(By::Css(format!(
            "button[aria-label='{label}'][aria-pressed='true']"
        )))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    Ok(())
}

async fn table_row(driver: &WebDriver, name: &str) -> Result<WebElement> {
    Ok(driver
        .query(By::Css(format!(
            "[data-testid='project-table-row'][aria-label='{name}'][aria-busy='false']"
        )))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?)
}

async fn exercise_table(session: &mut support::session::TestSession) -> Result<()> {
    select_view(session.driver(), "List view").await?;
    let alpha = table_row(session.driver(), "Alpha").await?;
    #[cfg(target_os = "linux")]
    ensure!(
        session.project_watch_registrations()? > 0,
        "visible Projects has no filesystem watches"
    );
    review_project_changes(session.driver(), &alpha).await?;
    for label in ["Create snapshot", "Open live"] {
        let button = alpha
            .find(By::Css(format!("a[aria-label='{label}']")))
            .await?;
        ensure!(
            button.text().await?.trim().is_empty(),
            "table action {label} must be icon only"
        );
    }
    ensure!(
        std::fs::read_to_string(session.data_root().join("config.toml"))?
            .contains("projects_view = \"table\""),
        "list choice was not saved in user settings"
    );
    for name in ["Beta", "Initial", "Missing"] {
        ensure!(
            table_row(session.driver(), name)
                .await?
                .find_all(By::Css("[aria-label='Create snapshot']"))
                .await?
                .is_empty(),
            "table project {name} offered an empty snapshot"
        );
    }
    let link = alpha
        .find(By::Css("[data-testid='project-table-name']"))
        .await?;
    let destination = link.attr("href").await?.context("row link destination")?;
    ensure!(
        destination.contains("/projects/live?path="),
        "row must expose a stable comparison URL"
    );
    ensure!(
        link.attr("draggable").await?.as_deref() == Some("false"),
        "row text must not drag a link"
    );
    support::evidence::capture(session.driver(), "projects-table-desktop", true).await?;
    assert_comparison_editor(session.driver(), "project-comparison-table").await?;
    exercise_comparison_retention(session, &alpha).await?;
    let destination = session.driver().current_url().await?.join(&destination)?;
    session.driver().goto(destination.as_str()).await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    assert_live_tab_presentation(session.driver()).await?;
    home(session.driver()).await?;
    table_row(session.driver(), "Alpha")
        .await?
        .find(By::Css("a[aria-label='Open live']"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    home(session.driver()).await?;
    table_row(session.driver(), "Alpha")
        .await?
        .find(By::Css("[data-testid='project-table-name']"))
        .await?
        .send_keys(thirtyfour::Key::Enter)
        .await
        .context("open project table name with Enter")?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    home(session.driver()).await?;
    restore_table_presentation(session).await
}

async fn review_project_changes(driver: &WebDriver, alpha: &WebElement) -> Result<()> {
    let changes = alpha
        .find(By::Css("[data-testid='project-table-changes']"))
        .await?;
    let summary = changes
        .find(By::Css("[role='img']"))
        .await?
        .attr("aria-label")
        .await?
        .context("changes summary")?;
    ensure!(
        summary.contains("unpushed commit")
            && summary.contains("tracked file")
            && summary.contains("untracked file"),
        "combined changes omitted a state: {summary}"
    );
    ensure!(
        changes.find_all(By::Css("svg")).await?.len() == 3,
        "combined changes must show ahead, tracked, and untracked icons"
    );
    ensure!(
        driver
            .find_all(By::Css(
                "[data-testid='project-table-unpushed'], [data-testid='project-table-rendered']"
            ))
            .await?
            .is_empty(),
        "obsolete columns remain"
    );
    driver
        .action_chain()
        .move_to_element_center(&changes)
        .perform()
        .await?;
    let tooltip = driver
        .query(By::Css("[role='tooltip'][aria-label='Project changes']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        tooltip.text().await?.contains("tracked file"),
        "changes tooltip omitted file detail"
    );
    support::evidence::capture(driver, "projects-changes-tooltip", true).await?;
    driver.action_chain().move_to(0, 0).perform().await?;
    changes.focus().await?;
    tooltip.wait_until().displayed().await?;
    driver
        .query(By::Css("thead button"))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .focus()
        .await
        .context("move focus out of the changes tooltip")?;
    Ok(())
}

async fn exercise_comparison_retention(
    session: &support::session::TestSession,
    alpha: &WebElement,
) -> Result<()> {
    let count = alpha.find(By::Css("a[aria-label='Open live']")).await?;
    count
        .click()
        .await
        .context("open unpushed comparison from table count")?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "committed-project-marker",
    )
    .await?;
    #[cfg(target_os = "linux")]
    support::wait::until(
        "Projects filesystem watches released off route",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok((session.project_watch_registrations()? == 0).then_some(())) },
    )
    .await?;
    session.driver().back().await?;
    ensure!(
        table_row(session.driver(), "Alpha").await? == *alpha,
        "comparison navigation replaced the project row"
    );
    Ok(())
}

async fn assert_live_tab_presentation(driver: &WebDriver) -> Result<()> {
    let tab = driver
        .find(By::Css("button[role='tab'][aria-selected='true']"))
        .await?;
    ensure!(
        tab.css_value("cursor").await? == "default",
        "tab still shows a drag cursor"
    );
    ensure!(
        !tab.text().await?.contains(": Unpushed commits"),
        "tab retained comparison suffix"
    );
    ensure!(
        tab.rect().await?.width < 200.0,
        "live tab is excessively wide"
    );
    Ok(())
}

async fn restore_table_presentation(session: &mut support::session::TestSession) -> Result<()> {
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(session.driver(), "projects-table-narrow", true).await?;
    assert_comparison_editor(session.driver(), "project-comparison-table-narrow").await?;
    session.restart().await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;
    table_row(session.driver(), "Alpha").await?;
    ensure!(
        session
            .driver()
            .find(By::Css("button[aria-label='List view']"))
            .await?
            .attr("aria-pressed")
            .await?
            .as_deref()
            == Some("true"),
        "saved list choice was not restored"
    );
    select_view(session.driver(), "Grid view").await?;
    card(session.driver(), "Alpha").await?;
    Ok(())
}

async fn assert_comparison_editor(driver: &WebDriver, evidence_name: &str) -> Result<()> {
    let trigger = table_row(driver, "Alpha")
        .await?
        .find(By::Css("button[aria-label^='Comparison branch']"))
        .await?;
    let panel_id = trigger
        .attr("aria-controls")
        .await?
        .context("comparison panel ID")?;
    trigger
        .send_keys(thirtyfour::Key::Enter)
        .await
        .with_context(|| format!("open comparison editor: {evidence_name}"))?;
    let panel = driver.find(By::Id(panel_id)).await?;
    panel.wait_until().displayed().await?;
    let fits = driver
        .execute(
            r"
        const panel = arguments[0];
        const input = panel.querySelector('input');
        const label = input.closest('label').querySelector('span');
        const text = document.createRange();
        text.selectNodeContents(label);
        return panel.scrollWidth <= panel.clientWidth + 1 &&
            Math.abs(text.getBoundingClientRect().left - input.getBoundingClientRect().left) < 1;
    ",
            vec![panel.to_json()?],
        )
        .await?
        .convert::<bool>()?;
    ensure!(
        fits,
        "comparison label, input, or helper text overflowed or lost left alignment"
    );
    support::evidence::capture(driver, evidence_name, true).await?;
    panel
        .find(By::Css("input"))
        .await?
        .send_keys(thirtyfour::Key::Escape)
        .await
        .with_context(|| format!("close comparison editor: {evidence_name}"))?;
    panel.wait_until().not_displayed().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_paginate_through_id_cursors_and_restore_the_page_size() -> Result<()> {
    support::run_test("viewer-project-pagination", |session| {
        Box::pin(run_pagination(session))
    })
    .await
}

async fn run_pagination(session: &mut support::session::TestSession) -> Result<()> {
    let fixture_home = std::env::var_os("HOME").context("fixture home")?;
    let root = tempfile::Builder::new()
        .prefix(".gtl-pages-")
        .tempdir_in(fixture_home)?;
    let changed = support::fixture::ProjectsFixture::create(root.path())?;
    let projects = (0..31_u8)
        .map(|index| {
            (
                format!(
                    "P{}{}",
                    char::from(b'A' + index / 26),
                    char::from(b'A' + index % 26)
                ),
                format!("Project {index:02}"),
                match index {
                    30 => changed.alpha.clone(),
                    29 => changed.beta.clone(),
                    _ => root.path().join(format!("project-{index:02}")),
                },
            )
        })
        .collect::<Vec<_>>();
    let catalogue = projects
        .iter()
        .map(|(id, title, path)| (id.as_str(), title.as_str(), path.as_path()))
        .collect::<Vec<_>>();
    session.catalogue.set_projects(&catalogue)?;
    session.write_user_config("projects_page_size = \"invalid\"\n")?;
    session.restart().await?;
    wait_for_project_count(session.driver(), 15).await?;
    wait_for_first_project(session.driver(), "Project 30").await?;
    exercise_column_sorting(session).await?;
    select_view(session.driver(), "Grid view").await?;
    wait_for_project_count(session.driver(), 15).await?;
    ensure!(
        session
            .driver()
            .find(By::Id("projects-page-size"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("15"),
        "malformed page size did not default to 15"
    );
    ensure!(
        session
            .driver()
            .find_all(By::Css("header.projects-header p"))
            .await?
            .is_empty(),
        "Projects header retained its summary"
    );
    exercise_last_page(session).await?;
    session
        .driver()
        .find(By::Css(
            "nav[aria-label='Projects pages'] [aria-label='First page']",
        ))
        .await?
        .click()
        .await?;
    wait_for_project_count(session.driver(), 15).await?;
    for (size, count) in [("10", 10), ("30", 30), ("15", 15)] {
        set_page_size(session.driver(), size).await?;
        wait_for_project_count(session.driver(), count).await?;
        ensure!(
            std::fs::read_to_string(session.data_root().join("config.toml"))?
                .contains(&format!("projects_page_size = {size}")),
            "page size was not persisted"
        );
    }
    support::evidence::capture(session.driver(), "projects-pagination-grid", true).await?;
    exercise_table_pages(session).await?;
    session.catalogue.set_projects(&catalogue[..4])?;
    session.driver().refresh().await?;
    wait_for_project_count(session.driver(), 4).await?;
    Ok(())
}

async fn exercise_last_page(session: &support::session::TestSession) -> Result<()> {
    wait_for_project_statuses(session.driver()).await?;
    let statuses = session.project_status_checks()?;
    let requests = session.project_requests()?;
    session
        .driver()
        .find(By::Css(
            "nav[aria-label='Projects pages'] [aria-label='Last page']",
        ))
        .await?
        .click()
        .await?;
    wait_for_project_count(session.driver(), 1).await?;
    ensure!(
        session.project_requests()? == requests + 1,
        "pagination must request exactly one project page"
    );
    wait_for_project_statuses(session.driver()).await?;
    support::wait::until(
        "Git status check for the visible project",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok((session.project_status_checks()? == statuses + 1).then_some(())) },
    )
    .await
}

async fn exercise_table_pages(session: &mut support::session::TestSession) -> Result<()> {
    select_view(session.driver(), "List view").await?;
    set_page_size(session.driver(), "10").await?;
    wait_for_project_count(session.driver(), 10).await?;
    session
        .driver()
        .find(By::Css(
            "nav[aria-label='Projects pages'] [aria-label='Next page']",
        ))
        .await?
        .send_keys(thirtyfour::Key::Enter)
        .await?;
    session
        .driver()
        .query(By::Css("output[aria-label='Page 2 of 4']"))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    wait_for_project_count(session.driver(), 10).await?;
    support::evidence::capture(session.driver(), "projects-pagination-table", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    let navigation = session
        .driver()
        .find(By::Css("nav[aria-label='Projects pages']"))
        .await?
        .rect()
        .await?;
    ensure!(
        navigation.x >= 0.0 && navigation.x + navigation.width <= 390.0,
        "pagination overflowed the narrow viewport"
    );
    support::evidence::capture(session.driver(), "projects-pagination-narrow", true).await?;
    exercise_retained_page(session).await?;
    session.restart().await?;
    wait_for_project_count(session.driver(), 10).await?;
    ensure!(
        session
            .driver()
            .find(By::Id("projects-page-size"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("10"),
        "saved page size was not restored"
    );
    wait_for_sort(session.driver(), "Project", "ascending").await?;
    wait_for_first_project(session.driver(), "Project 00").await?;
    click_sort(session.driver(), "Changes").await?;
    wait_for_first_project(session.driver(), "Project 30").await?;
    Ok(())
}

async fn exercise_retained_page(session: &mut support::session::TestSession) -> Result<()> {
    let driver = session.driver();
    driver.set_window_rect(0, 0, 390, 550).await?;
    let row = table_row(driver, "Project 19").await?;
    row.scroll_into_view().await?;
    let content = driver.find(By::Id("projects-content")).await?;
    let scroll = content.prop("scrollTop").await?;
    ensure!(
        scroll.as_deref() != Some("0"),
        "retention fixture did not scroll"
    );
    support::selectors::by_test_id(driver, gtl_web_contracts::test_ids::VIEWER_MENU_TRIGGER)
        .await?
        .click()
        .await?;
    driver
        .query(By::Css("[aria-label='User settings']"))
        .and_displayed()
        .first()
        .await?
        .click()
        .await?;
    content.wait_until().not_displayed().await?;
    ensure!(
        driver.title().await?.contains("Settings"),
        "hidden Projects replaced the route title"
    );
    home(driver).await?;
    ensure!(
        table_row(driver, "Project 19").await? == row,
        "Settings navigation replaced the retained row"
    );
    ensure!(
        content.prop("scrollTop").await? == scroll,
        "Settings navigation reset project scroll"
    );
    let fixture =
        support::fixture::ViewerFixture::create_named(session.data_root(), "retention-live-view")?;
    fixture.forward_live_view()?;
    support::wait_for_active_diff(driver, "retention-live-view", "alpha-v1").await?;
    ensure!(
        !row.is_displayed().await?,
        "Projects remained visible over the diff"
    );
    home(driver).await?;
    driver
        .query(By::Css("output[aria-label='Page 2 of 4']"))
        .and_displayed()
        .first()
        .await?;
    ensure!(
        table_row(driver, "Project 19").await? == row,
        "diff navigation replaced the retained row"
    );
    ensure!(
        content.prop("scrollTop").await? == scroll,
        "diff navigation reset project scroll"
    );
    support::evidence::capture(driver, "projects-retained-page", true).await?;
    driver.back().await?;
    support::wait_for_active_diff(driver, "retention-live-view", "alpha-v1").await?;
    session.restart_server().await?;
    content.wait_until().stale().await?;
    home(session.driver()).await?;
    session
        .driver()
        .query(By::Css("output[aria-label='Page 1 of 4']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    Ok(())
}

async fn wait_for_project_count(driver: &WebDriver, expected: usize) -> Result<()> {
    support::wait::until(
        "bounded project page",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok((driver
                .find_all(By::Css("[data-project-card], [data-project-row]"))
                .await?
                .len()
                == expected)
                .then_some(()))
        },
    )
    .await
}

async fn set_page_size(driver: &WebDriver, value: &str) -> Result<()> {
    let select = driver
        .query(By::Id("projects-page-size"))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    thirtyfour::components::SelectElement::new(&select)
        .await?
        .select_by_value(value)
        .await?;
    Ok(())
}

async fn wait_for_project_statuses(driver: &WebDriver) -> Result<()> {
    support::wait::until(
        "visible project statuses",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find_all(By::Css("[aria-label='Loading Git status']"))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
    .await
}

async fn verify_watched_status(driver: &WebDriver, path: &std::path::Path) -> Result<()> {
    let before = card(driver, "Beta").await?.rect().await?;
    let changed = path.join("watch-status.txt");
    std::fs::write(&changed, "watch status change\n")?;
    support::wait::until(
        "watched untracked status",
        Duration::from_secs(10),
        || async {
            Ok(card(driver, "Beta")
                .await?
                .text()
                .await?
                .contains("[?]")
                .then_some(()))
        },
    )
    .await?;
    let after = card(driver, "Beta").await?.rect().await?;
    ensure!(
        (after.height - before.height).abs() < 1.0,
        "status refresh resized project card"
    );
    std::fs::remove_file(changed)?;
    support::wait::until("watched clean status", Duration::from_secs(10), || async {
        Ok(card(driver, "Beta")
            .await?
            .text()
            .await?
            .contains("[✓]")
            .then_some(()))
    })
    .await?;
    Ok(())
}

async fn click_sort(driver: &WebDriver, column: &str) -> Result<()> {
    driver
        .query(By::Css("thead button"))
        .ignore_errors(true)
        .with_text(StringMatch::new(column))
        .and_displayed()
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    Ok(())
}

async fn wait_for_sort(driver: &WebDriver, column: &str, direction: &str) -> Result<()> {
    driver
        .query(By::Css(format!("thead th[aria-sort='{direction}'] button")))
        .ignore_errors(true)
        .with_text(StringMatch::new(column))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        driver.find_all(By::Css("thead th[aria-sort]")).await?.len() == 1,
        "only the active column may announce a sort direction"
    );
    Ok(())
}

async fn exercise_column_sorting(session: &mut support::session::TestSession) -> Result<()> {
    select_view(session.driver(), "List view").await?;
    wait_for_sort(session.driver(), "Changes", "descending").await?;
    ensure!(
        session
            .driver()
            .find_all(By::Id("projects-sort"))
            .await?
            .is_empty(),
        "the standalone sort dropdown remains"
    );
    session
        .driver()
        .find(By::Css(
            "nav[aria-label='Projects pages'] [aria-label='Last page']",
        ))
        .await?
        .click()
        .await?;
    wait_for_project_count(session.driver(), 1).await?;
    click_sort(session.driver(), "Project").await?;
    wait_for_first_project(session.driver(), "Project 00").await?;
    wait_for_sort(session.driver(), "Project", "ascending").await?;
    click_sort(session.driver(), "Changes").await?;
    wait_for_first_project(session.driver(), "Project 30").await?;
    click_sort(session.driver(), "Project").await?;
    wait_for_first_project(session.driver(), "Project 00").await?;
    session
        .driver()
        .query(By::Css("thead button"))
        .ignore_errors(true)
        .with_text(StringMatch::new("Project"))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .send_keys(thirtyfour::Key::Enter)
        .await
        .context("reverse Project sorting with Enter")?;
    wait_for_first_project(session.driver(), "Project 30").await?;
    wait_for_sort(session.driver(), "Project", "descending").await?;
    support::evidence::capture(session.driver(), "projects-column-sorting", true).await?;
    session.restart().await?;
    wait_for_sort(session.driver(), "Project", "descending").await?;
    wait_for_first_project(session.driver(), "Project 30").await?;
    click_sort(session.driver(), "Changes").await?;
    wait_for_sort(session.driver(), "Changes", "descending").await?;
    click_sort(session.driver(), "Changes").await?;
    wait_for_first_project(session.driver(), "Project 29").await?;
    wait_for_sort(session.driver(), "Changes", "ascending").await?;
    click_sort(session.driver(), "Branch").await?;
    wait_for_sort(session.driver(), "Branch", "ascending").await?;
    click_sort(session.driver(), "Branch").await?;
    wait_for_sort(session.driver(), "Branch", "descending").await?;
    click_sort(session.driver(), "Project").await?;
    wait_for_first_project(session.driver(), "Project 00").await?;
    wait_for_sort(session.driver(), "Project", "ascending").await?;
    Ok(())
}

async fn wait_for_first_project(driver: &WebDriver, name: &str) -> Result<()> {
    driver
        .query(By::Css(
            "[data-project-card]:first-child, [data-project-row]:first-child",
        ))
        .ignore_errors(true)
        .with_attribute("aria-label", StringMatch::new(name))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    Ok(())
}
