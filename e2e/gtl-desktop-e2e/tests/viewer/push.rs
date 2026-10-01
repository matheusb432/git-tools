use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, wait};

#[tokio::test(flavor = "multi_thread")]
async fn global_viewer_push_opt_out_is_independent_of_cli_and_survives_restart() -> Result<()> {
    support::run_test("push-preference", |session| {
        Box::pin(async move {
            let directory = tempfile::Builder::new()
                .prefix(".gtl-push-preference-")
                .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
            let fixture = support::fixture::PushFixture::create(directory.path())?;
            fixture.forward_snapshot(session.data_root())?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;

            super::settings::open_settings(driver).await?;
            support::click(driver, By::Css("[data-settings-section='git']")).await?;
            assert_confirmation_options(driver, false, false).await?;
            support::click(driver, By::Id("settings-push-confirmation")).await?;
            wait_for_saved_confirmation(session.data_root(), false, true).await?;
            assert_confirmation_options(driver, true, false).await?;
            support::click(driver, By::Css("button[aria-label='Back']")).await?;
            support::visible(driver, By::Css("#viewer-push-trigger:enabled")).await?;
            push_with_keyboard(driver).await?;
            wait_for_review(driver, &fixture.latest).await?;
            support::click(
                driver,
                By::XPath(
                    "//dialog[@id='viewer-push-confirmation']//button[normalize-space()='Cancel']",
                ),
            )
            .await?;

            support::click(driver, By::Css("button[aria-label='Settings']")).await?;
            support::click(driver, By::Css("[data-settings-section='git']")).await?;
            support::click(driver, By::Id("settings-viewer-push-confirmation")).await?;
            wait_for_saved_confirmation(session.data_root(), false, false).await?;
            support::evidence::capture(driver, "viewer-push-preference", true).await?;

            session.restart().await?;
            let driver = session.driver();
            fixture.forward_snapshot(session.data_root())?;
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
            support::click(driver, By::Css("button[aria-label='Settings']")).await?;
            support::click(driver, By::Css("[data-settings-section='git']")).await?;
            assert_confirmation_options(driver, true, true).await?;
            support::click(driver, By::Css("button[aria-label='Back']")).await?;
            support::visible(driver, By::Css("#viewer-push-trigger:enabled")).await?;
            push_with_keyboard(driver).await?;
            support::dismiss_toast(driver, "Push completed.").await?;
            ensure!(
                fixture.remote_head()? == fixture.latest,
                "global viewer opt-out did not push the reviewed snapshot"
            );
            ensure!(
                driver
                    .find(By::Css("#viewer-push-confirmation[open]"))
                    .await
                    .is_err(),
                "global viewer opt-out showed confirmation"
            );
            ensure!(
                fixture.repository.join("untracked.txt").exists(),
                "global viewer opt-out changed untracked work"
            );
            Ok(())
        })
    })
    .await
}

async fn assert_confirmation_options(
    driver: &WebDriver,
    cli_skip: bool,
    viewer_skip: bool,
) -> Result<()> {
    for (id, expected) in [
        ("settings-push-confirmation", cli_skip),
        ("settings-viewer-push-confirmation", viewer_skip),
    ] {
        let checkbox = support::visible(driver, By::Id(id)).await?;
        ensure!(
            checkbox.is_selected().await? == expected,
            "{id} did not retain its independent preference"
        );
    }
    Ok(())
}

async fn wait_for_saved_confirmation(
    data_root: &std::path::Path,
    cli_required: bool,
    viewer_required: bool,
) -> Result<()> {
    let endpoint = gtl_local_transport::LocalEndpoint::from_root(data_root)?;
    wait::until(
        "push confirmation preference saved",
        wait::ASSERTION_TIMEOUT,
        || async {
            let mut client = gtl_client::ViewerClient::connect(&endpoint).await?;
            let preferences = client.get_settings().await?.push_confirmation;
            Ok((preferences.cli_required == cli_required
                && preferences.viewer_required == viewer_required)
                .then_some(()))
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn user_pushes_exactly_the_reviewed_commit() -> Result<()> {
    support::run_test("push", |session| {
        Box::pin(async move {
            let directory = tempfile::Builder::new()
                .prefix(".gtl-push-")
                .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
            let fixture = support::fixture::PushFixture::create(directory.path())?;
            fixture.register_project(session.data_root())?;
            fixture.forward_snapshot(session.data_root())?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;

            support::visible(driver, By::Css("#viewer-push-trigger:enabled")).await?;
            wait_for_snapshot_status(driver, true).await?;
            support::click(driver, By::Css("button[aria-label='Modified files']")).await?;
            support::wait_for_active_diff(driver, "push-review", "keep local").await?;
            wait_for_snapshot_status(driver, true).await?;
            support::click(driver, By::Css("button[aria-label='Modified files']")).await?;
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
            support::click(driver, By::Css("button[aria-label$=': push earlier']")).await?;
            support::visible(
                driver,
                By::Css("button[aria-label$=': push earlier'][aria-pressed='true']:enabled"),
            )
            .await?;
            support::click(driver, By::Css("button[aria-label$=': push latest']")).await?;
            support::visible(
                driver,
                By::Css("button[aria-label$=': push latest'][aria-pressed='true']:enabled"),
            )
            .await?;
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
            support::visible(driver, By::Css("#viewer-push-trigger:enabled")).await?;
            push_with_keyboard(driver).await?;
            wait_for_review(driver, &fixture.latest).await?;

            support::click(
                driver,
                By::XPath(
                    "//dialog[@id='viewer-push-confirmation']//button[normalize-space()='Cancel']",
                ),
            )
            .await?;
            support::click(driver, By::Id("commits-sidebar-toggle")).await?;
            support::visible(
                driver,
                By::Css("#commits-sidebar-toggle[aria-pressed='false']"),
            )
            .await?;
            support::click(driver, By::Id("review-push-trigger")).await?;
            wait_for_review(driver, &fixture.latest).await?;

            fixture.add_newer()?;
            support::visible(
                driver,
                By::XPath(
                    "//dialog[@id='viewer-push-confirmation']//button[normalize-space()='Push']",
                ),
            )
            .await?
            .send_keys(Key::Enter)
            .await?;
            support::dismiss_toast(driver, "Push completed.").await?;
            ensure!(
                fixture.remote_head()? == fixture.latest,
                "the push did not stop at the reviewed commit"
            );
            ensure!(
                fixture.repository.join("untracked.txt").exists(),
                "the push changed untracked work"
            );
            support::click(driver, By::Id("commits-sidebar-toggle")).await?;
            support::visible(driver, By::Css("#viewer-push-trigger:disabled")).await?;
            wait_for_snapshot_status(driver, false).await?;

            push_without_confirmation(driver, &fixture, session.data_root()).await
        })
    })
    .await
}

async fn push_without_confirmation(
    driver: &WebDriver,
    fixture: &support::fixture::PushFixture,
    data_root: &std::path::Path,
) -> Result<()> {
    support::click(driver, By::Css("a[aria-label='Projects']")).await?;
    support::click(
        driver,
        By::Css("tr[aria-label='push-review'] button[aria-label='Edit push-review']"),
    )
    .await?;
    select_project_opt_out(driver).await?;
    let newer = fixture.current_head()?;
    fixture.forward_snapshot(data_root)?;
    support::wait_for_active_diff(driver, "push-review", "push-newer-marker").await?;
    support::click(driver, By::Css("a[aria-label='Projects']")).await?;
    save_project_opt_out(driver).await?;
    support::evidence::capture(driver, "project-push-preference", true).await?;
    support::click(
        driver,
        By::XPath("//*[@role='tab' and contains(., 'push-review')]"),
    )
    .await?;
    support::wait_for_active_diff(driver, "push-review", "push-newer-marker").await?;
    support::visible(driver, By::Css("#review-push-trigger:enabled")).await?;
    push_with_keyboard(driver).await?;
    support::dismiss_toast(driver, "Push completed.").await?;
    ensure!(
        fixture.remote_head()? == newer,
        "project opt-out did not push the reviewed snapshot"
    );
    ensure!(
        driver
            .find(By::Css("#viewer-push-confirmation[open]"))
            .await
            .is_err(),
        "project opt-out showed confirmation"
    );
    Ok(())
}

async fn push_with_keyboard(driver: &WebDriver) -> Result<()> {
    driver
        .action_chain()
        .key_down(Key::Control)
        .send_keys(Key::Enter)
        .key_up(Key::Control)
        .perform()
        .await?;
    Ok(())
}

async fn wait_for_snapshot_status(driver: &WebDriver, unpushed: bool) -> Result<()> {
    wait::until("snapshot push status", wait::ASSERTION_TIMEOUT, || async {
        let has_status = driver
            .find(By::Css(".review-action-dock [role='status']"))
            .await?
            .prop("textContent")
            .await?
            .is_some_and(|text| !text.is_empty());
        Ok((has_status == unpushed).then_some(()))
    })
    .await
}

const PROJECT_OPT_OUT: &str = "#project-edit-push-no-confirmation:enabled";

async fn select_project_opt_out(driver: &WebDriver) -> Result<()> {
    let checkbox = support::visible(driver, By::Css(PROJECT_OPT_OUT)).await?;
    ensure!(
        !checkbox.is_selected().await?,
        "viewer confirmation opt-out must default off"
    );
    checkbox.click().await?;
    Ok(())
}

async fn save_project_opt_out(driver: &WebDriver) -> Result<()> {
    ensure!(
        support::visible(driver, By::Css(PROJECT_OPT_OUT))
            .await?
            .is_selected()
            .await?,
        "leaving Projects discarded the unsaved opt-out"
    );
    support::click(
        driver,
        By::XPath("//dialog[@id='project-edit-dialog']//button[normalize-space()='Save']"),
    )
    .await?;
    wait::until(
        "project push preference saved",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find(By::Id("project-edit-dialog"))
                .await
                .is_err()
                .then_some(()))
        },
    )
    .await
}

async fn wait_for_review(driver: &WebDriver, commit: &str) -> Result<()> {
    support::visible(driver, By::Css("#viewer-push-confirmation[open]")).await?;
    support::evidence::capture(driver, "push-confirmation-compact", true).await?;
    support::click(driver, By::XPath(
        "//dialog[@id='viewer-push-confirmation']//summary[normalize-space()='Details & command']",
    )).await?;
    wait::until(
        "review of the selected SHA and its destination",
        wait::ASSERTION_TIMEOUT,
        || async {
            let review = driver
                .find(By::Css("#viewer-push-confirmation[open]"))
                .await?
                .text()
                .await?;
            Ok((review.contains(commit)
                && review.contains("Branch")
                && review.contains("Remote")
                && review.contains("origin"))
            .then_some(()))
        },
    )
    .await?;
    support::evidence::capture(driver, "push-confirmation-details", true).await?;
    Ok(())
}
