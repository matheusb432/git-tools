use anyhow::Context as _;
use gtl_web_contracts::test_ids;

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_refreshes_and_restores_a_saved_live_diff() -> anyhow::Result<()> {
    support::run_test("viewer-live-lifecycle", |session| {
        Box::pin(async move {
            let fixture = support::fixture::ViewerFixture::create(session.data_root())?;
            fixture.forward_live_view()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v1")
                .await
                .context("show the forwarded live diff")?;

            fixture.commit_alpha_v2()?;
            support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_REFRESH)
                .await?
                .click()
                .await
                .context("refresh the live diff")?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
                .await
                .context("show the refreshed live diff")?;

            session
                .restart()
                .await
                .context("restart saved live viewer")?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
                .await
                .context("restore the refreshed live diff")
        })
    })
    .await
}
