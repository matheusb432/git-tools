mod support;

use anyhow::Context as _;

#[tokio::test(flavor = "multi_thread")]
async fn viewer_live_lifecycle() -> anyhow::Result<()> {
    support::run_test("viewer-live-lifecycle", |session| {
        Box::pin(async move {
            let fixture = support::fixture::ViewerFixture::create()?;
            fixture.forward_live_view()?;
            support::assert_forwarded_live_view(session, &fixture)
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
            support::delete_and_restore_empty_state(session)
                .await
                .context("delete and restore empty state")
        })
    })
    .await
}
