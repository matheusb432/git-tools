use std::time::Duration;

use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_models::recipes::RecipeLabel;
use gtl_wire::{
    v1,
    viewer::{ViewerActiveState, ViewerTab, ViewerTabState},
};

use super::{ServerHarness, TestResult, live_views::ready_shell};

type Client = v1::viewer_service_client::ViewerServiceClient<tonic::transport::Channel>;

fn changed_repository(path: std::path::PathBuf) -> TestRepository {
    let repository = TestRepository::init(path);
    repository.write("work.txt", "base\n");
    repository.commit_all("base");
    repository.write("work.txt", "changed\n");
    repository
}

fn repository_name(tab: &ViewerTab) -> String {
    match &tab.label {
        RecipeLabel::Named { name } => name.to_string(),
        RecipeLabel::Repository { repository }
        | RecipeLabel::Changes { repository, .. }
        | RecipeLabel::Compared { repository, .. } => repository.to_string(),
    }
}

/// Waits until no restored tab is still waiting for its first render.
async fn every_tab_ready(client: &mut Client) -> TestResult {
    while !ready_shell(client)
        .await?
        .tabs
        .iter()
        .all(|tab| tab.state == ViewerTabState::Ready)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

#[tokio::test]
async fn open_tabs_survive_a_restart_with_their_order_pins_and_active_tab() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = changed_repository(directory.path().join("first"));
    let second = changed_repository(directory.path().join("second"));
    let database = SqliteAppState::open(directory.path())?;
    super::seed_live_tabs(
        &database,
        [
            super::working_tree_recipe(first.root()),
            super::working_tree_recipe(second.root()),
        ],
    )?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let (first_tab, second_tab) = (shell.tabs[0].id, shell.tabs[1].id);
    client
        .set_viewer_tab_pinned(v1::SetViewerTabPinnedRequest {
            tab_id: second_tab.into(),
            pinned: true,
        })
        .await?;
    client
        .activate_viewer_tab(v1::ActivateViewerTabRequest {
            tab_id: first_tab.into(),
        })
        .await?;
    server.stop().await?;

    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;

    assert_eq!(
        shell.tabs.iter().map(repository_name).collect::<Vec<_>>(),
        ["second", "first"]
    );
    assert_eq!(
        shell.tabs.iter().map(|tab| tab.pinned).collect::<Vec<_>>(),
        [true, false]
    );
    assert!(shell.tabs.iter().all(|tab| tab.live));
    assert_eq!(
        shell.tabs[0].label,
        RecipeLabel::Compared {
            repository: gtl_models::paths::ProjectName::try_new("second")?,
            base: gtl_models::git::GitRevision::head(),
            head: gtl_models::recipes::RecipeLabelHead::WorkingTree,
        },
        "a restored tab shows its saved label before it renders again"
    );
    let ViewerActiveState::Ready { view } = shell.active else {
        return Err("expected the restored active tab to render".into());
    };
    assert_eq!(view.identity.tab_id, shell.tabs[1].id);
    tokio::time::timeout(Duration::from_secs(10), every_tab_ready(&mut client)).await??;
    server.stop().await?;
    Ok(())
}
