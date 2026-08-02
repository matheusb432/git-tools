use anyhow::Context as _;

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn viewer_live_lifecycle() -> anyhow::Result<()> {
    support::run_test("viewer-live-lifecycle", |session| {
        Box::pin(async move {
            let fixture = support::fixture::ViewerFixture::create(session.data_root())?;
            fixture.forward_live_view()?;
            support::assert_forwarded_live_view(session, "alpha-v1")
                .await
                .context("assert forwarded live view")?;
            support::assert_configured_editor_launch(session, &fixture)
                .await
                .context("assert configured editor launch")?;
            support::assert_first_paint(session)
                .await
                .context("assert first diff-row paint")?;
            support::select_and_restore_split_layout(session)
                .await
                .context("select and restore split layout")?;
            fixture.commit_alpha_v2()?;
            support::refresh_and_assert_alpha_v2(session)
                .await
                .context("refresh and assert alpha-v2")?;
            support::select_commit_patch_and_restore_range(session)
                .await
                .context("select standalone commit patch and restore range")?;
            support::assert_mobile_navigation(session)
                .await
                .context("assert mobile navigation")?;
            support::assert_overlapping_live_updates(session, &fixture)
                .await
                .context("assert overlapping live updates")?;
            support::delete_temporary_live_views(session)
                .await
                .context("remove temporary overlapping live views")?;

            session
                .restart()
                .await
                .context("restart saved live viewer")?;
            support::assert_restarted_live_view(session, "alpha-v2")
                .await
                .context("assert saved live view after restart")?;

            fixture.make_repository_unavailable()?;
            support::refresh_and_assert_unavailable(session)
                .await
                .context("assert unavailable repository state")?;
            fixture.restore_repository()?;
            fixture.forward_live_view()?;
            support::assert_restarted_live_view(session, "alpha-v2")
                .await
                .context("recover saved live view without duplicate tabs")?;

            support::delete_and_restore_empty_state(session)
                .await
                .context("delete saved live view through keyboard confirmation")?;
            session
                .restart()
                .await
                .context("restart viewer after deleting saved live view")?;
            support::assert_durable_empty_state(session)
                .await
                .context("assert saved live view remains deleted")
        })
    })
    .await
}
