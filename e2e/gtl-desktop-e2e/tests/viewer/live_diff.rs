use anyhow::Result;
use gtl_web_contracts::test_ids;
use thirtyfour::By;

use crate::support::{self, fixture::ViewerFixture};

#[tokio::test(flavor = "multi_thread")]
async fn a_tab_made_live_follows_new_commits_across_server_and_viewer_restarts() -> Result<()> {
    support::run_test("live-diff", |session| {
        Box::pin(async move {
            let fixture = ViewerFixture::create(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v1").await?;
            support::click(
                session.driver(),
                By::Css(test_ids::VIEWER_LIVE_TOGGLE.selector()),
            )
            .await?;
            support::visible(
                session.driver(),
                By::Css(format!(
                    "{}[aria-pressed='true']",
                    test_ids::VIEWER_LIVE_TOGGLE.selector()
                )),
            )
            .await?;

            fixture.commit_alpha_v2()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2").await?;

            session.restart_server().await?;
            fixture.commit_extra()?;
            support::wait_for_active_diff(session.driver(), "live-view", "additional-live-marker")
                .await?;

            session.restart().await?;
            support::click(
                session.driver(),
                By::Css("[role='tab'][title*='live-view']"),
            )
            .await?;
            support::wait_for_active_diff(session.driver(), "live-view", "additional-live-marker")
                .await
        })
    })
    .await
}
