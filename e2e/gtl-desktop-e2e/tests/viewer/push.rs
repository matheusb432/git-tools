use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, wait};

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
            observe_commit_switches(driver).await?;
            support::click(driver, By::Css("button[aria-label$=': push earlier']")).await?;
            support::visible(driver, By::Css("button[aria-label$=': push earlier'][aria-pressed='true']:enabled")).await?;
            support::click(driver, By::Css("button[aria-label$=': push latest']")).await?;
            support::visible(driver, By::Css("button[aria-label$=': push latest'][aria-pressed='true']:enabled")).await?;
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;
            let flickered = driver.execute("cancelAnimationFrame(window.commitSwitchObservation.frame); return window.commitSwitchObservation.flickered;", Vec::new()).await?.convert::<bool>()?;
            ensure!(!flickered, "switching commits flashed a spinner or disabled Push");
            push_with_keyboard(driver).await?;
            wait_for_review(driver, &fixture.latest).await?;

            support::click(
                driver,
                By::XPath("//dialog[@id='viewer-push-confirmation']//button[normalize-space()='Cancel']"),
            ).await?;
            support::click(driver, By::Id("commits-sidebar-toggle")).await?;
            support::visible(driver, By::Css("#commits-sidebar-toggle[aria-pressed='false']")).await?;
            support::click(driver, By::Id("review-push-trigger")).await?;
            wait_for_review(driver, &fixture.latest).await?;

            fixture.add_newer()?;
            support::click(
                driver,
                By::XPath(
                    "//dialog[@id='viewer-push-confirmation']//button[starts-with(normalize-space(), 'Push ')]",
                ),
            )
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

            support::click(driver, By::Css("a[aria-label='Projects']")).await?;
            support::click(driver, By::Css("tr[aria-label='push-review'] button[aria-label^='Project settings,']")).await?;
            enable_project_opt_out(driver).await?;
            support::evidence::capture(driver, "project-push-preference", true).await?;
            let newer = fixture.current_head()?;
            fixture.forward_snapshot(session.data_root())?;
            support::wait_for_active_diff(driver, "push-review", "push-newer-marker").await?;
            support::visible(driver, By::Css("#review-push-trigger:enabled")).await?;
            push_with_keyboard(driver).await?;
            support::dismiss_toast(driver, "Push completed.").await?;
            ensure!(fixture.remote_head()? == newer, "project opt-out did not push the reviewed snapshot");
            ensure!(driver.find(By::Css("#viewer-push-confirmation[open]")).await.is_err(), "project opt-out showed confirmation");
            Ok(())
        })
    })
    .await
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

async fn enable_project_opt_out(driver: &WebDriver) -> Result<()> {
    let setting = By::Css("input[id$='-push-no-confirmation']:enabled");
    let checkbox = support::visible(driver, setting.clone()).await?;
    ensure!(
        !checkbox.is_selected().await?,
        "viewer confirmation opt-out must default off"
    );
    checkbox.click().await?;
    wait::until(
        "project push preference saved",
        wait::ASSERTION_TIMEOUT,
        || async {
            let checkbox = driver.find(setting.clone()).await?;
            Ok((checkbox.is_enabled().await? && checkbox.is_selected().await?).then_some(()))
        },
    )
    .await
}

async fn observe_commit_switches(driver: &WebDriver) -> Result<()> {
    driver
        .execute(
            r#"
        const observation = { flickered: false, frame: null };
        window.commitSwitchObservation = observation;
        function sample() {
            observation.flickered ||= Boolean(
                document.querySelector('#viewer-push-trigger:disabled')
                || document.querySelector('.diff-commit-card [role="status"]')
            );
            observation.frame = requestAnimationFrame(sample);
        }
        sample();
    "#,
            Vec::new(),
        )
        .await?;
    Ok(())
}

async fn wait_for_review(driver: &WebDriver, commit: &str) -> Result<()> {
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
    .await
}
