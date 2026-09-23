use anyhow::{Context as _, Result, ensure};
use thirtyfour::{
    By, WebDriver, WebElement, prelude::ElementQueryable as _, stringmatch::StringMatch,
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_confirms_an_exact_commit_push_and_recovers_from_a_rewritten_review() -> Result<()> {
    support::run_test("viewer-push", |session| Box::pin(journey(session))).await
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_push_eligibility_follows_each_selected_commit_after_partial_pushes() -> Result<()>
{
    support::run_test("snapshot-push", |session| {
        Box::pin(snapshot_journey(session))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn rewritten_snapshot_warns_without_blocking_pushes_of_surviving_commits() -> Result<()> {
    support::run_test("rewritten-snapshot-push", |session| {
        Box::pin(rewritten_snapshot_journey(session))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn confirmed_pushes_leave_diffs_usable_and_show_each_operation_progress() -> Result<()> {
    support::run_test("parallel-snapshot-push", |session| {
        Box::pin(parallel_snapshot_journey(session))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn same_repository_pushes_wait_for_the_previous_diff() -> Result<()> {
    support::run_test("queued-snapshot-push", |session| {
        Box::pin(queued_snapshot_journey(session))
    })
    .await
}

async fn queued_snapshot_journey(session: &mut support::session::TestSession) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix(".gtl-queued-push-")
        .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
    let fixture = support::fixture::PushFixture::create(directory.path())?;
    let gate = fixture.hold_remote_push()?;
    session
        .catalogue
        .set_projects(&[("PSH", "Push review", &fixture.repository)])?;
    let driver = session.driver();
    home(driver).await?;
    driver.refresh().await?;
    element(driver, "a[aria-label='Create snapshot']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "Push review", "push-latest-marker").await?;
    let first_tab_id = active_tab_id(driver).await?;
    home(driver).await?;
    element(driver, "a[aria-label='Open live']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
    let second_tab_id = active_tab_id(driver).await?;
    ensure!(
        first_tab_id != second_tab_id,
        "live diff reused the snapshot tab"
    );
    element(driver, &format!("#{first_tab_id}"))
        .await?
        .click()
        .await?;
    select_commit(driver, "push first").await?;
    review(driver, "#viewer-push-trigger", &fixture.first).await?;
    click_confirm(driver).await?;
    support::wait::until(
        "first remote push entered its gate",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok(gate.entered().exists().then_some(())) },
    )
    .await?;

    element(driver, &format!("#{second_tab_id}"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
    review(driver, "#viewer-push-trigger", &fixture.latest).await?;
    click_confirm(driver).await?;
    driver
        .query(By::Css(
            "#viewer-push-trigger[aria-label='Waiting...'][aria-busy='true']",
        ))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    ensure!(
        driver.find_all(By::Css("dialog[open]")).await?.is_empty(),
        "queued push kept the confirmation modal open"
    );

    element(driver, &format!("#{first_tab_id}"))
        .await?
        .click()
        .await?;
    driver
        .query(By::Css("#viewer-push-trigger[aria-busy='true']"))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    gate.release()?;
    success(driver).await?;
    element(driver, "[data-testid='toast-dismiss']")
        .await?
        .click()
        .await?;
    element(driver, &format!("#{second_tab_id}"))
        .await?
        .click()
        .await?;
    success(driver).await?;
    ensure!(
        fixture.remote_head()? == fixture.latest,
        "queued push did not reach the second reviewed SHA"
    );
    driver
        .query(By::Id("viewer-push-trigger"))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    Ok(())
}

async fn parallel_snapshot_journey(session: &mut support::session::TestSession) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix(".gtl-parallel-push-")
        .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
    let first = support::fixture::PushFixture::create(&directory.path().join("first"))?;
    let second = support::fixture::PushFixture::create(&directory.path().join("second"))?;
    let gate = first.hold_remote_push()?;
    session.catalogue.set_projects(&[
        ("ONE", "First push", &first.repository),
        ("TWO", "Second push", &second.repository),
    ])?;
    let driver = session.driver();
    home(driver).await?;
    driver.refresh().await?;

    element(
        driver,
        "[data-project-card][aria-label='First push'] a[aria-label='Create snapshot']",
    )
    .await?
    .click()
    .await?;
    support::wait_for_active_diff(driver, "First push", "push-latest-marker").await?;
    let first_tab_id = active_tab_id(driver).await?;
    home(driver).await?;
    element(
        driver,
        "[data-project-card][aria-label='Second push'] a[aria-label='Create snapshot']",
    )
    .await?
    .click()
    .await?;
    support::wait_for_active_diff(driver, "Second push", "push-latest-marker").await?;
    let second_tab_id = active_tab_id(driver).await?;
    element(driver, &format!("#{first_tab_id}"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "First push", "push-latest-marker").await?;
    review(driver, "#viewer-push-trigger", &first.latest).await?;
    click_confirm(driver).await?;
    support::wait::until(
        "first remote push entered its gate",
        support::wait::ASSERTION_TIMEOUT,
        || async { Ok(gate.entered().exists().then_some(())) },
    )
    .await?;
    driver
        .query(By::Css("#viewer-push-trigger[aria-busy='true']"))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    ensure!(
        driver.find_all(By::Css("dialog[open]")).await?.is_empty(),
        "confirmation kept the viewer modal while the first push ran"
    );

    element(driver, &format!("#{second_tab_id}"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "Second push", "push-latest-marker").await?;
    review(driver, "#viewer-push-trigger", &second.latest).await?;
    confirm(driver).await?;
    ensure!(
        second.remote_head()? == second.latest,
        "second diff could not push while the first push was running"
    );

    element(driver, &format!("#{first_tab_id}"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "First push", "push-latest-marker").await?;
    driver
        .query(By::Css("#viewer-push-trigger[aria-busy='true']"))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    gate.release()?;
    success(driver).await?;
    ensure!(
        first.remote_head()? == first.latest,
        "first push did not finish"
    );
    disabled_push(driver).await?;
    Ok(())
}

async fn rewritten_snapshot_journey(session: &mut support::session::TestSession) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix(".gtl-rewritten-snapshot-")
        .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
    let fixture = support::fixture::PushFixture::create(directory.path())?;
    fixture.append_commit("push removed")?;
    session
        .catalogue
        .set_projects(&[("PSH", "Push review", &fixture.repository)])?;
    let driver = session.driver();
    home(driver).await?;
    driver.refresh().await?;
    element(driver, "a[aria-label='Create snapshot']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "Push review", "push removed").await?;
    // The button's last successful check predates this external rewrite.
    element(driver, "#viewer-push-trigger").await?;
    fixture.soft_reset(&fixture.latest)?;
    element(driver, "#viewer-push-trigger")
        .await?
        .click()
        .await?;
    warning(driver, "no longer in this branch").await?;

    select_commit(driver, "push latest").await?;
    review(driver, "#viewer-push-trigger", &fixture.latest).await?;
    confirm(driver).await?;
    ensure!(
        fixture.remote_head()? == fixture.latest,
        "surviving commit could not be pushed"
    );
    disabled_push(driver).await?;

    select_commit(driver, "push latest").await?;
    driver
        .query(By::Css(
            "#viewer-push-trigger[title*='no longer in this branch']",
        ))
        .and_not_enabled()
        .first()
        .await?;
    ensure!(
        driver
            .find_all(By::Css("[data-testid='toast']"))
            .await?
            .is_empty(),
        "browsing an unavailable commit enqueued a toast"
    );
    select_commit(driver, "push first").await?;
    disabled_push(driver).await?;
    Ok(())
}

async fn warning(driver: &WebDriver, message: &str) -> Result<()> {
    driver
        .query(By::Css("[data-testid='toast'][role='alert']"))
        .and_displayed()
        .with_text(StringMatch::new(message).partial())
        .first()
        .await?;
    ensure!(
        driver.find_all(By::Css("dialog[open]")).await?.is_empty(),
        "refusal opened a modal"
    );
    element(driver, "[data-testid='toast-dismiss']")
        .await?
        .click()
        .await?;
    Ok(())
}

async fn snapshot_journey(session: &mut support::session::TestSession) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix(".gtl-snapshot-push-")
        .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
    let fixture = support::fixture::PushFixture::create(directory.path())?;
    let third = fixture.append_commit("push third")?;
    fixture.append_commit("push fourth")?;
    fixture.append_commit("push fifth")?;
    let sixth = fixture.append_commit("push sixth")?;
    session
        .catalogue
        .set_projects(&[("PSH", "Push review", &fixture.repository)])?;
    let driver = session.driver();
    home(driver).await?;
    driver.refresh().await?;
    element(driver, "a[aria-label='Create snapshot']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "Push review", "push sixth").await?;

    select_commit(driver, "push latest").await?;
    review(driver, "#viewer-push-trigger", &fixture.latest).await?;
    confirm(driver).await?;
    ensure!(
        fixture.remote_head()? == fixture.latest,
        "partial push used the wrong SHA"
    );
    disabled_push(driver).await?;
    select_commit(driver, "push first").await?;
    disabled_push(driver).await?;

    select_commit(driver, "push third").await?;
    review(driver, "#viewer-push-trigger", &third).await?;
    confirm(driver).await?;
    ensure!(
        fixture.remote_head()? == third,
        "next selected commit was not pushed"
    );
    disabled_push(driver).await?;

    // Clicking the selected commit returns to the same six-commit snapshot.
    select_commit(driver, "push third").await?;
    support::wait_for_active_diff(driver, "Push review", "push sixth").await?;
    review(driver, "#viewer-push-trigger", &sixth).await?;
    confirm(driver).await?;
    ensure!(
        fixture.remote_head()? == sixth,
        "full snapshot did not push its original tip"
    );
    disabled_push(driver).await?;
    support::wait_for_active_diff(driver, "Push review", "push sixth").await?;
    support::evidence::capture(driver, "snapshot-fully-pushed", true).await?;
    Ok(())
}

async fn select_commit(driver: &WebDriver, subject: &str) -> Result<()> {
    element(
        driver,
        &format!("[aria-label='Commits'] button[aria-label*='{subject}']"),
    )
    .await?
    .click()
    .await?;
    Ok(())
}

async fn disabled_push(driver: &WebDriver) -> Result<()> {
    driver
        .query(By::Css(
            "#viewer-push-trigger[title='No unpushed commits through this SHA']",
        ))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    Ok(())
}

async fn confirm(driver: &WebDriver) -> Result<()> {
    click_confirm(driver).await?;
    success(driver).await?;
    element(driver, "[data-testid='toast-dismiss']")
        .await?
        .click()
        .await?;
    Ok(())
}

async fn click_confirm(driver: &WebDriver) -> Result<()> {
    element(
        driver,
        "#viewer-push-confirmation button.control-button-variant-warning",
    )
    .await?
    .click()
    .await?;
    Ok(())
}

async fn element(driver: &WebDriver, selector: &str) -> Result<WebElement> {
    Ok(driver
        .query(By::Css(selector))
        .and_displayed()
        .and_enabled()
        .first()
        .await?)
}

async fn active_tab_id(driver: &WebDriver) -> Result<String> {
    element(driver, ".viewer-tab[data-active='true'] [role='tab']")
        .await?
        .attr("id")
        .await?
        .context("active diff tab ID")
}

async fn review(driver: &WebDriver, trigger: &str, commit: &str) -> Result<WebElement> {
    element(driver, trigger).await?.click().await?;
    let dialog = element(driver, "#viewer-push-confirmation[open]").await?;
    ensure!(
        dialog
            .find(By::Css(".push-confirmation-command"))
            .await?
            .text()
            .await?
            .contains(commit),
        "confirmation omitted exact SHA"
    );
    ensure!(
        dialog.text().await?.contains("origin/main"),
        "confirmation omitted upstream destination"
    );
    Ok(dialog)
}

async fn cancel(driver: &WebDriver) -> Result<()> {
    element(
        driver,
        "#viewer-push-confirmation [data-dialog-initial-focus]",
    )
    .await?
    .click()
    .await?;
    support::wait::until(
        "confirmation closed",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find_all(By::Css("#viewer-push-confirmation[open]"))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
    .await
}

async fn home(driver: &WebDriver) -> Result<()> {
    element(driver, "a[aria-label='Projects']")
        .await?
        .click()
        .await?;
    element(driver, "#projects-heading").await?;
    Ok(())
}

async fn journey(session: &mut support::session::TestSession) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix(".gtl-push-")
        .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
    let fixture = support::fixture::PushFixture::create(directory.path())?;
    session.write_user_config("theme = \"mirage\"\n[push]\nconfirm = false\n")?;
    session
        .catalogue
        .set_projects(&[("PSH", "Push review", &fixture.repository)])?;
    home(session.driver()).await?;
    session.driver().refresh().await?;

    review_entry_points(session.driver(), &fixture).await?;

    home(session.driver()).await?;
    element(session.driver(), "a[aria-label='Create snapshot']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(session.driver(), "Push review", "push-latest-marker").await?;
    review(session.driver(), "#viewer-push-trigger", &fixture.latest).await?;
    fixture.add_newer()?;
    ensure!(
        element(
            session.driver(),
            "#viewer-push-confirmation .push-confirmation-command"
        )
        .await?
        .text()
        .await?
        .contains(&fixture.latest),
        "review changed after a new commit"
    );
    element(
        session.driver(),
        "#viewer-push-confirmation button.control-button-variant-warning",
    )
    .await?
    .click()
    .await?;
    success(session.driver()).await?;
    support::evidence::capture(session.driver(), "push-success-toast", true).await?;
    element(session.driver(), "[data-testid='toast-dismiss']")
        .await?
        .click()
        .await?;
    ensure!(
        fixture.remote_head()? == fixture.latest,
        "push included a newer commit"
    );
    ensure!(
        fixture.repository.join("untracked.txt").exists(),
        "push changed untracked work"
    );
    support::wait_for_active_diff(session.driver(), "Push review", "push-latest-marker").await?;

    recover_and_finish(session.driver(), &fixture).await
}

async fn recover_and_finish(
    driver: &WebDriver,
    fixture: &support::fixture::PushFixture,
) -> Result<()> {
    home(driver).await?;
    let trigger = "#project-push-table-PSH";
    element(driver, trigger).await?.click().await?;
    element(driver, "#viewer-push-confirmation[open]").await?;
    fixture.rewrite()?;
    element(
        driver,
        "#viewer-push-confirmation button.control-button-variant-warning",
    )
    .await?
    .click()
    .await?;
    let failure = element(driver, "[data-testid='toast'][role='alert']").await?;
    ensure!(
        failure.text().await?.contains("no longer in this branch"),
        "rewrite error was not relayed"
    );
    support::evidence::capture(driver, "push-rewritten-error", true).await?;
    ensure!(
        driver.find_all(By::Css("dialog[open]")).await?.is_empty(),
        "refusal left a modal open"
    );
    element(driver, "[data-testid='toast-dismiss']")
        .await?
        .click()
        .await?;
    ensure!(
        fixture.remote_head()? == fixture.latest,
        "rejected push changed remote history"
    );
    element(driver, "a[aria-label='Open live']")
        .await?
        .click()
        .await?;
    driver
        .query(By::Css("[aria-label='Commits']"))
        .and_displayed()
        .with_text(StringMatch::new("push rewritten").partial())
        .first()
        .await?;
    element(driver, "#viewer-push-trigger")
        .await?
        .click()
        .await?;
    element(
        driver,
        "#viewer-push-confirmation button.control-button-variant-warning",
    )
    .await?
    .click()
    .await?;
    success(driver).await?;
    home(driver).await?;
    driver
        .query(By::Css("#project-push-table-PSH"))
        .and_displayed()
        .and_not_enabled()
        .first()
        .await?;
    support::evidence::capture(driver, "push-nothing-pending", true).await?;
    element(driver, "a[aria-label='Open live']")
        .await?
        .click()
        .await?;
    driver.set_window_rect(0, 0, 390, 800).await?;
    element(driver, "nav button[aria-label='Modified files']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "push-review", "keep local").await?;
    Ok(())
}

async fn success(driver: &WebDriver) -> Result<()> {
    driver
        .query(By::Css("[data-testid='toast'][role='status']"))
        .and_displayed()
        .with_text(StringMatch::new("Push completed.").partial())
        .first()
        .await?;
    ensure!(
        driver.find_all(By::Css("dialog[open]")).await?.is_empty(),
        "success left a modal open"
    );
    Ok(())
}

async fn review_entry_points(
    driver: &WebDriver,
    fixture: &support::fixture::PushFixture,
) -> Result<()> {
    review(driver, "#project-push-card-PSH", &fixture.latest).await?;
    support::evidence::capture(driver, "push-confirmation-wide", true).await?;
    driver.set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(driver, "push-confirmation-narrow", true).await?;
    driver.set_window_rect(0, 0, 1600, 900).await?;
    cancel(driver).await?;

    element(driver, "button[aria-label='List view']")
        .await?
        .click()
        .await?;
    let row = element(driver, "[data-testid='project-table-row']").await?;
    let push = element(driver, "#project-push-table-PSH").await?;
    let accent = push.css_value("color").await?;
    for action in row
        .find_all(By::Css("td:last-child :is(button:not(:disabled), a[href])"))
        .await?
    {
        if action.is_displayed().await? {
            ensure!(
                action.css_value("color").await? == accent,
                "enabled table action is not accent colored"
            );
            ensure!(
                action.rect().await?.x <= push.rect().await?.x,
                "Push is not the last row action"
            );
        }
    }
    review(driver, "#project-push-table-PSH", &fixture.latest).await?;
    cancel(driver).await?;

    element(driver, "a[aria-label='Open live']")
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
    element(
        driver,
        "[aria-label='Commits'] header button[aria-label='Modified files']",
    )
    .await?;
    support::evidence::capture(driver, "push-shelf-wide", true).await?;
    for width in [390, 380] {
        driver.set_window_rect(0, 0, width, 800).await?;
        let push = element(driver, "#viewer-push-trigger").await?;
        ensure!(
            push.text().await?.is_empty(),
            "narrow Push button kept its label"
        );
        let header = element(driver, ".diff-workspace-titlebar").await?;
        let bounds = header.rect().await?;
        for button in header.find_all(By::Css("button")).await? {
            if button.is_displayed().await? {
                let rect = button.rect().await?;
                ensure!(
                    rect.x >= bounds.x && rect.x + rect.width <= bounds.x + bounds.width + 1.0,
                    "header action was clipped at {width}px"
                );
            }
        }
        let modified = element(
            driver,
            "nav[aria-label='Viewer panels'] button[aria-label='Modified files']",
        )
        .await?;
        let background = modified.css_value("background-color").await?;
        modified.click().await?;
        support::wait_for_active_diff(driver, "push-review", "keep local").await?;
        let selected = element(
            driver,
            "nav button[aria-label='Modified files'][aria-pressed='true']",
        )
        .await?;
        ensure!(
            selected.css_value("background-color").await? != background,
            "selected Modified files has no background tint"
        );
        support::evidence::capture(driver, &format!("push-modified-{width}"), true).await?;
        selected.click().await?;
        support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
        support::evidence::capture(driver, &format!("push-header-{width}"), true).await?;
    }
    driver.set_window_rect(0, 0, 1600, 900).await?;
    element(
        driver,
        "[aria-label='Commits'] button[aria-label*='push first'][aria-pressed='false']",
    )
    .await?
    .click()
    .await?;
    element(
        driver,
        "[aria-label='Commits'] button[aria-label*='push first'][aria-pressed='true']",
    )
    .await?;
    review(driver, "#viewer-push-trigger", &fixture.first).await?;
    cancel(driver).await?;

    Ok(())
}
