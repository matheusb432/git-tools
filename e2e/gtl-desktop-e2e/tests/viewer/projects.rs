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
    session
        .driver()
        .find(By::Css("button[aria-label='Refresh projects']"))
        .await?
        .click()
        .await?;
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
    let initial_unpushed = action(session.driver(), "Initial", "Branch changes").await?;
    ensure!(
        !initial_unpushed.is_enabled().await?,
        "initial repository must not offer unpushed commits"
    );
    ensure!(
        card(session.driver(), "Initial")
            .await?
            .text()
            .await?
            .contains("Local comparison branch 'main' is missing"),
        "missing comparison branch must be explained"
    );
    ensure!(
        !action(session.driver(), "Missing", "Local changes")
            .await?
            .is_enabled()
            .await?,
        "missing repository action must be disabled"
    );
    #[cfg(unix)]
    assert_refresh_dimensions(session).await?;
    support::evidence::capture(session.driver(), "projects-desktop", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    #[cfg(unix)]
    assert_refresh_dimensions(session).await?;
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
    action(session.driver(), "Alpha", "Branch changes")
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
    let content = session.driver().find(By::Css("main")).await?.text().await?;
    ensure!(
        !content.contains("refreshed-project-marker"),
        "branch comparison included uncommitted work"
    );
    fixture.restore_upstream()?;
    Ok(())
}

async fn review_comparisons(
    session: &support::session::TestSession,
    fixture: &support::fixture::ProjectsFixture,
) -> Result<()> {
    action(session.driver(), "Alpha", "Local changes")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "untracked-project-marker",
    )
    .await?;
    ensure!(
        session
            .driver()
            .find(By::Css("main"))
            .await?
            .text()
            .await?
            .contains("local-project-marker"),
        "local comparison omitted tracked work"
    );
    fixture.change_untracked()?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "refreshed-project-marker",
    )
    .await?;
    home(session.driver()).await?;
    action(session.driver(), "Alpha", "Unpushed commits")
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
        !session
            .driver()
            .find(By::Css("main"))
            .await?
            .text()
            .await?
            .contains("refreshed-project-marker"),
        "unpushed comparison included local changes"
    );
    home(session.driver()).await?;
    action(session.driver(), "Beta", "Local changes")
        .await?
        .click()
        .await?;
    session
        .driver()
        .query(By::Css("main"))
        .ignore_errors(true)
        .with_text(StringMatch::new("No changes").partial())
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    home(session.driver()).await?;
    support::wait::until(
        "changes to review stay ahead of the most recently rendered clean project",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            let cards = session
                .driver()
                .find_all(By::Css("[data-project-card]"))
                .await?;
            let Some(first) = cards.first() else {
                return Ok(None);
            };
            Ok((first.attr("aria-label").await?.as_deref() == Some("Alpha")).then_some(()))
        },
    )
    .await?;
    action(session.driver(), "Initial", "Local changes")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-initial",
        "initial-project-marker",
    )
    .await?;
    Ok(())
}

async fn restore_comparisons(session: &mut support::session::TestSession) -> Result<()> {
    home(session.driver()).await?;
    action(session.driver(), "Alpha", "Local changes")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "refreshed-project-marker",
    )
    .await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("[role='tab'][title*='projects-alpha']"))
            .await?
            .len()
            == 2,
        "reopening created a duplicate comparison tab"
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
            "[role='tab'][title*='projects-alpha'][title*='Local changes']",
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
        "refreshed-project-marker",
    )
    .await?;
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
        .query(By::Css(format!("article[aria-label='{name}']")))
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
            session
                .driver()
                .find(By::Css("button[aria-label='Refresh projects']"))
                .await?
                .click()
                .await?;
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
                .and_displayed()
                .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
                .first()
                .await?;
            Ok(())
        })
    })
    .await
}

#[cfg(unix)]
fn same_bounds(before: &thirtyfour::ElementRect, after: &thirtyfour::ElementRect) -> bool {
    [
        before.x - after.x,
        before.y - after.y,
        before.width - after.width,
        before.height - after.height,
    ]
    .into_iter()
    .all(|difference| difference.abs() < 0.5)
}

#[cfg(unix)]
async fn assert_refresh_dimensions(session: &support::session::TestSession) -> Result<()> {
    let refresh = session
        .driver()
        .query(By::Css("button[aria-label='Refresh projects']"))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    let before = refresh.rect().await?;
    let paused = session.pause_server()?;
    refresh.click().await?;
    session
        .driver()
        .query(By::Css(
            "button[aria-label='Refresh projects'][aria-busy='true']",
        ))
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(25))
        .first()
        .await?;
    let during = refresh.rect().await?;
    ensure!(
        same_bounds(&before, &during),
        "Refresh moved while loading: {before:?} -> {during:?}"
    );
    support::evidence::capture(session.driver(), "projects-refresh-loading", true).await?;
    drop(paused);
    session
        .driver()
        .query(By::Css("button[aria-label='Refresh projects']"))
        .and_enabled()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        same_bounds(&before, &refresh.rect().await?),
        "Refresh moved after loading"
    );
    Ok(())
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
            "[data-testid='project-table-row'][aria-label='{name}']"
        )))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?)
}

async fn exercise_table(session: &mut support::session::TestSession) -> Result<()> {
    select_view(session.driver(), "List view").await?;
    let alpha = table_row(session.driver(), "Alpha").await?;
    ensure!(
        alpha.text().await?.contains("[!?]"),
        "table changed the grid status glyphs"
    );
    ensure!(
        std::fs::read_to_string(session.data_root().join("config.toml"))?
            .contains("projects_view = \"table\""),
        "list choice was not saved in user settings"
    );
    ensure!(
        !table_row(session.driver(), "Initial")
            .await?
            .find(By::Css("button[aria-label='Branch changes']"))
            .await?
            .is_enabled()
            .await?,
        "unborn repository comparison must be disabled"
    );
    let link = alpha
        .find(By::Css("[data-testid='project-table-name']"))
        .await?;
    let destination = link.attr("href").await?.context("row link destination")?;
    ensure!(
        destination.contains("/projects/unpushed?path="),
        "row must expose a stable comparison URL"
    );
    ensure!(
        link.attr("draggable").await?.as_deref() == Some("false"),
        "row text must not drag a link"
    );
    support::evidence::capture(session.driver(), "projects-table-desktop", true).await?;
    assert_comparison_editor(session.driver(), "project-comparison-table").await?;
    let count = alpha
        .find(By::Css("[data-testid='project-table-unpushed']"))
        .await?;
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
    session.driver().back().await?;
    table_row(session.driver(), "Alpha").await?;
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
        .find(By::Css("a[aria-label='Local changes']"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(
        session.driver(),
        "projects-alpha",
        "untracked-project-marker",
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
    trigger.send_keys(thirtyfour::Key::Enter).await?;
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
        .await?;
    panel.wait_until().not_displayed().await?;
    Ok(())
}
