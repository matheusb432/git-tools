use anyhow::Result;
use thirtyfour::{By, Key};

use crate::support::{self, fixture::ViewerFixture};

#[tokio::test(flavor = "multi_thread")]
async fn a_tab_made_live_follows_new_commits_across_server_and_viewer_restarts() -> Result<()> {
    support::run_test("live-diff", |session| {
        Box::pin(async move {
            let fixture = ViewerFixture::create(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v1").await?;
            fixture.commit_alpha_v2()?;
            let tab = support::visible(session.driver(), By::Css("[role='tab'][title*='live-view']")).await?;
            support::context_click_element(session.driver(), &tab).await?;
            support::click(session.driver(), By::XPath("//*[@role='menu' and @aria-label='Tab actions']//*[@role='menuitem' and normalize-space(.)='Refresh']")).await?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2").await?;
            support::context_click_element(session.driver(), &tab).await?;
            support::click(session.driver(), By::XPath("//*[@role='menu' and @aria-label='Tab actions']//*[@role='menuitemcheckbox' and normalize-space(.)='Live']")).await?;
            support::context_click_element(session.driver(), &tab).await?;
            support::visible(
                session.driver(),
                By::XPath("//*[@role='menu' and @aria-label='Tab actions']//*[@role='menuitemcheckbox' and normalize-space(.)='Live' and @aria-checked='true']"),
            )
            .await?;
            session.driver().action_chain().send_keys(Key::Escape).perform().await?;

            fixture.commit_extra()?;
            support::wait_for_active_diff(session.driver(), "live-view", "additional-live-marker").await?;

            session.restart_server().await?;
            support::wait_for_active_diff(session.driver(), "live-view", "additional-live-marker")
                .await?;

            session.restart().await?;
            support::click(
                session.driver(),
                By::Css("[role='tab'][title*='live-view']"),
            )
            .await?;
            support::wait_for_active_diff(session.driver(), "live-view", "additional-live-marker")
                .await?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2").await
        })
    })
    .await
}
