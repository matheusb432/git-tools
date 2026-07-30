mod support;

#[tokio::test(flavor = "multi_thread")]
async fn offline_artifact_fold_all() -> anyhow::Result<()> {
    let spec = support::Spec::start("offline-artifact-fold-all").await?;
    let outcome = async {
        let repository = support::repository_with_worktree_change().await?;
        let artifact_url = support::render_raw_diff(&repository).await?;
        support::set_mobile_viewport(&spec.session.page).await?;
        support::goto(&spec.session.page, &artifact_url).await?;

        let files = spec.session.page.locator("details.file");
        let expanded_files = spec.session.page.locator("details.file[open]");
        anyhow::ensure!(
            support::count(&files, "count artifact file sections").await? > 0,
            "artifact rendered no file sections"
        );
        anyhow::ensure!(
            support::count(&expanded_files, "count expanded artifact file sections").await? > 0,
            "artifact started with no expanded file section"
        );

        support::click(
            &support::get_button(&spec.session.page, "View settings"),
            "open raw mobile view settings",
        )
        .await?;
        support::click(
            &support::get_button(&spec.session.page, "Collapse or expand all files"),
            "collapse files from raw mobile view settings",
        )
        .await?;

        support::wait_until_every_file_is_collapsed(&spec.session.page).await
    }
    .await;
    spec.finish(outcome).await
}
