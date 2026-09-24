use anyhow::Result;
use thirtyfour::By;

use crate::support::{self, fixture::ViewerFixture};

#[tokio::test(flavor = "multi_thread")]
async fn live_diff_follows_new_commits_across_server_and_viewer_restarts() -> Result<()> {
    support::run_test("live-diff", |session| {
        Box::pin(async move {
            let fixture = ViewerFixture::create(session.data_root())?;
            fixture.forward_live_view()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v1").await?;

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
